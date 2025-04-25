use std::{cell::RefCell,sync::atomic::{self, AtomicBool}};
use bitcoin::{block::Header, consensus, BlockHash, OutPoint, Transaction, TxOut, Txid};
use entry::{ChangeRecordRune, Entry, HeaderValue, InscriptionEntry, InscriptionNumber, MyOutPoints, OutPointValue, RuneBalances, RuneEntry, RuneIdValue, SatPointValue, SequenceNumbers, TxidValue};
use event::Events;
use ic_stable_structures::{StableCell, StableBTreeMap};
use ordinals::{Charm, RuneId, SatPoint};
use utxo_entry::UtxoEntry;
use anyhow::anyhow;

use crate::{
  chain::Chain, config::Config, inscriptions::{envelope::ParsedEnvelope, Inscription, InscriptionId, InscriptionQuery, InscriptionResp}, memory::{
    get_virtual_memory, VMemory, CONFIG_MEMORY_ID, HEIGHT_TO_BLOCK_HEADER_MEMORY_ID, HEIGHT_TO_CHANGE_RECORD_RUNE_MEMORY_ID, HEIGHT_TO_LAST_SEQUENCE_NUMBER_MEMORY_ID, HEIGHT_TO_STATISTIC_RESERVED_RUNES_MEMORY_ID, HEIGHT_TO_STATISTIC_RUNES_MEMORY_ID, HOME_INSCRIPTIONS_MEMORY_ID, INSCRIPTION_ID_TO_SEQUENCE_NUMBER_MEMORY_ID, INSCRIPTION_NUMBER_TO_SEQUENCE_NUMBER_MEMORY_ID, OUTPOINT_TO_HEIGHT_MEMORY_ID, OUTPOINT_TO_RUNE_BALANCES_MEMORY_ID, OUTPOINT_TO_UTXO_ENTRY_MEMORY_ID, RUNE_ID_TO_RUNE_ENTRY_MEMORY_ID, RUNE_TO_RUNE_ID_MEMORY_ID, SAT_TO_SATPOINT_MEMORY_ID, SAT_TO_SEQUENCE_NUMBERS_MEMORY_ID, SCRIPT_PUBKEY_TO_OUTPOINTS_MEMORY_ID, SEQUENCE_NUMBER_TO_CHILDRENS_MEMORY_ID, SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY_MEMORY_ID, SEQUENCE_NUMBER_TO_RUNE_ID_MEMORY_ID, SEQUENCE_NUMBER_TO_SATPOINT_MEMORY_ID, STATISTIC_TO_COUNT_MEMORY_ID, TRANSACTION_ID_TO_RUNE_MEMORY_ID, TRANSACTION_ID_TO_TRANSACTION_MEMORY_ID
  }, timestamp, unbound_outpoint, Result
};

pub mod entry;
mod event;
mod reorg;
pub mod updater;
mod utxo_entry;
mod lot;

#[derive(Copy, Clone)]
pub(crate) enum Statistic {
  // Schema = 0,
  BlessedInscriptions = 1,
  Commits = 2,
  CursedInscriptions = 3,
  // IndexAddresses = 4,
  // IndexInscriptions = 5,
  // IndexRunes = 6,
  // IndexSats = 7,
  // IndexTransactions = 8,
  // InitialSyncTime = 9,
  LostSats = 10,
  OutputsTraversed = 11,
  // ReservedRunes = 12,
  // Runes = 13,
  SatRanges = 14,
  UnboundInscriptions = 16,
  // LastSavepointHeight = 17,
}

impl Statistic {
  fn key(self) -> u64 {
    self.into()
  }
}

impl From<Statistic> for u64 {
  fn from(statistic: Statistic) -> Self {
    statistic as u64
  }
}

pub struct Index {
  genesis_block_coinbase_transaction: Transaction,
  genesis_block_coinbase_txid: Txid,
  index_addresses: bool,
  index_inscriptions: bool,
  index_runes: bool,
  index_sats: bool,
  index_transactions: bool, 
  chain: Option<Chain>,
  integration_test: bool,
  first_index_height: u32,
}

impl Index {
  // pub fn new(chain: Chain) -> Self {
  //   let genesis_block_coinbase_transaction = chain.genesis_block().coinbase().unwrap().clone();
  //   Self {
  //     genesis_block_coinbase_txid: genesis_block_coinbase_transaction.compute_txid(),
  //     genesis_block_coinbase_transaction,
  //     index_addresses: false,
  //     index_inscriptions: true,
  //     index_runes: true,
  //     index_sats: false,
  //     index_transactions: false,
  //     chain: Some(chain),
  //     integration_test: false,
  //     first_index_height: chain.first_inscription_height(),
  //   }
  // }

  pub fn from_config(config: &Config) -> Self {
    let index_addresses = config.index_addresses.unwrap_or_default();
    let index_inscriptions = config.index_inscriptions.unwrap_or_default();
    let index_runes = config.index_runes.unwrap_or_default();
    let index_sats = config.index_sats.unwrap_or_default();
    let index_transactions = config.index_transactions.unwrap_or_default();
    let chain = match config.network {
      ic_cdk::api::management_canister::bitcoin::BitcoinNetwork::Mainnet => Chain::Mainnet,
      ic_cdk::api::management_canister::bitcoin::BitcoinNetwork::Testnet => Chain::Testnet,
      ic_cdk::api::management_canister::bitcoin::BitcoinNetwork::Regtest => Chain::Regtest,
        
    };
    let genesis_block_coinbase_transaction = chain.genesis_block().coinbase().unwrap().clone();
    let mut index =     Self {
      genesis_block_coinbase_txid: genesis_block_coinbase_transaction.compute_txid(),
      genesis_block_coinbase_transaction,
      index_addresses,
      index_inscriptions,
      index_runes,
      index_sats,
      index_transactions,
      chain: Some(chain),
      integration_test: false,
      first_index_height: 0,
    };
    let first_index_height = if index_sats || index_addresses {
      0
    } else if index_inscriptions {
      index.first_inscription_height()
    } else if index_runes {
      index.first_rune_height()
    } else {
      u32::MAX
    };
    index.set_first_index_height(first_index_height);
    index
  }
  pub fn chain(&self) -> Chain {
    self.chain.unwrap()
  }

  
  /// Unlike normal outpoints, which are added to index on creation and removed
  /// when spent, the UTXO entry for special outpoints may be updated.
  ///
  /// The special outpoints are the null outpoint, which receives lost sats,
  /// and the unbound outpoint, which receives unbound inscriptions.
  pub fn is_special_outpoint(outpoint: OutPoint) -> bool {
    outpoint == OutPoint::null() || outpoint == unbound_outpoint()
  }

  pub fn get_first_index_height(&self) -> u32 {
    self.first_index_height
  }

  pub fn first_inscription_height(&self) -> u32 {
    if self.integration_test {
      0
    } else {
      self.chain.unwrap().first_inscription_height()
    }
  }

  pub fn first_rune_height(&self) -> u32 {
    if self.integration_test {
      0
    } else {
      self.chain.unwrap().first_rune_height()
    }
  }

  pub fn set_first_index_height(&mut self, height: u32) {
    self.first_index_height = height;
  }

  pub fn have_full_utxo_index(&self) -> bool {
    self.first_index_height == 0
  }

  pub fn has_sat_index(&self) -> bool {
    self.index_sats
  }

}

thread_local! {
  static CONFIG: RefCell<StableCell<Config, VMemory>> = RefCell::new(
    StableCell::init(get_virtual_memory(CONFIG_MEMORY_ID), Config::default()).unwrap()
  );
  /// multimap memories
  static SAT_TO_SEQUENCE_NUMBERS: RefCell<StableBTreeMap<u64, SequenceNumbers, VMemory>> = RefCell::new(
    StableBTreeMap::init(get_virtual_memory(SAT_TO_SEQUENCE_NUMBERS_MEMORY_ID))
  );
  static SAT_TO_SATPOINT: RefCell<StableBTreeMap<u64, SatPointValue, VMemory>> = RefCell::new(
    StableBTreeMap::init(get_virtual_memory(SAT_TO_SATPOINT_MEMORY_ID))
  );
  static SEQUENCE_NUMBER_TO_CHILDRENS: RefCell<StableBTreeMap<u32, SequenceNumbers, VMemory>> = RefCell::new(
    StableBTreeMap::init(get_virtual_memory(SEQUENCE_NUMBER_TO_CHILDRENS_MEMORY_ID))
  );
  static SCRIPT_PUBKEY_TO_OUTPOINTS: RefCell<StableBTreeMap<Vec<u8>, MyOutPoints, VMemory>> = RefCell::new(
    StableBTreeMap::init(get_virtual_memory(SCRIPT_PUBKEY_TO_OUTPOINTS_MEMORY_ID))
  );
  /// map memories
  static HEIGHT_TO_BLOCK_HEADER: RefCell<StableBTreeMap<u32, HeaderValue, VMemory>> = RefCell::new(
    StableBTreeMap::init(get_virtual_memory(HEIGHT_TO_BLOCK_HEADER_MEMORY_ID))
  );
  static HEIGHT_TO_LAST_SEQUENCE_NUMBER: RefCell<StableBTreeMap<u32, u32, VMemory>> = RefCell::new(
    StableBTreeMap::init(get_virtual_memory(HEIGHT_TO_LAST_SEQUENCE_NUMBER_MEMORY_ID))
  );
  static SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY: RefCell<StableBTreeMap<u32, InscriptionEntry, VMemory>> = RefCell::new(
    StableBTreeMap::init(get_virtual_memory(SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY_MEMORY_ID))
  );
  static INSCRIPTION_ID_TO_SEQUENCE_NUMBER: RefCell<StableBTreeMap<InscriptionId, u32, VMemory>> = RefCell::new(
    StableBTreeMap::init(get_virtual_memory(INSCRIPTION_ID_TO_SEQUENCE_NUMBER_MEMORY_ID))
  );
  static TRANSACTION_ID_TO_TRANSACTION: RefCell<StableBTreeMap<TxidValue, Vec<u8>, VMemory>> = RefCell::new(
    StableBTreeMap::init(get_virtual_memory(TRANSACTION_ID_TO_TRANSACTION_MEMORY_ID))
  );
  static INSCRIPTION_NUMBER_TO_SEQUENCE_NUMBER: RefCell<StableBTreeMap<InscriptionNumber, u32, VMemory>> = RefCell::new(
    StableBTreeMap::init(get_virtual_memory(INSCRIPTION_NUMBER_TO_SEQUENCE_NUMBER_MEMORY_ID))
  );
  static HOME_INSCRIPTIONS: RefCell<StableBTreeMap<u32, InscriptionId, VMemory>> = RefCell::new(
    StableBTreeMap::init(get_virtual_memory(HOME_INSCRIPTIONS_MEMORY_ID))
  );
  // static HEIGHT_TO_EVENTS: RefCell<StableBTreeMap<u32, Events, VMemory>> = RefCell::new(
  //   StableBTreeMap::init(get_virtual_memory(HEIGHT_TO_EVENTS_MEMORY_ID))
  // );
  static STATISTIC_TO_COUNT: RefCell<StableBTreeMap<u64, u64, VMemory>> = RefCell::new(
    StableBTreeMap::init(get_virtual_memory(STATISTIC_TO_COUNT_MEMORY_ID))
  );
  static OUTPOINT_TO_UTXO_ENTRY: RefCell<StableBTreeMap<OutPointValue, UtxoEntry, VMemory>> = RefCell::new(
    StableBTreeMap::init(get_virtual_memory(OUTPOINT_TO_UTXO_ENTRY_MEMORY_ID))
  );
  static SEQUENCE_NUMBER_TO_SATPOINT: RefCell<StableBTreeMap<u32, SatPointValue, VMemory>> = RefCell::new(
    StableBTreeMap::init(get_virtual_memory(SEQUENCE_NUMBER_TO_SATPOINT_MEMORY_ID))
  );
  static SEQUENCE_NUMBER_TO_RUNE_ID: RefCell<StableBTreeMap<u32, RuneIdValue, VMemory>> = RefCell::new(
    StableBTreeMap::init(get_virtual_memory(SEQUENCE_NUMBER_TO_RUNE_ID_MEMORY_ID))
  );
  static RUNE_ID_TO_RUNE_ENTRY: RefCell<StableBTreeMap<RuneIdValue, RuneEntry, VMemory>> = RefCell::new(
    StableBTreeMap::init(get_virtual_memory(RUNE_ID_TO_RUNE_ENTRY_MEMORY_ID))
  );
  static OUTPOINT_TO_RUNE_BALANCES: RefCell<StableBTreeMap<OutPointValue, RuneBalances, VMemory>> = RefCell::new(
    StableBTreeMap::init(get_virtual_memory(OUTPOINT_TO_RUNE_BALANCES_MEMORY_ID))
  );
  static OUTPOINT_TO_HEIGHT: RefCell<StableBTreeMap<OutPointValue, u32, VMemory>> = RefCell::new(
    StableBTreeMap::init(get_virtual_memory(OUTPOINT_TO_HEIGHT_MEMORY_ID))
  );
  static HEIGHT_TO_CHANGE_RECORD_RUNE: RefCell<StableBTreeMap<u32, ChangeRecordRune, VMemory>> = RefCell::new(
    StableBTreeMap::init(get_virtual_memory(HEIGHT_TO_CHANGE_RECORD_RUNE_MEMORY_ID))
  );

  static RUNE_TO_RUNE_ID: RefCell<StableBTreeMap<u128, RuneIdValue, VMemory>> = RefCell::new(
    StableBTreeMap::init(get_virtual_memory(RUNE_TO_RUNE_ID_MEMORY_ID))
  );

  static TRANSACTION_ID_TO_RUNE: RefCell<StableBTreeMap<TxidValue, u128, VMemory>> = RefCell::new(
    StableBTreeMap::init(get_virtual_memory(TRANSACTION_ID_TO_RUNE_MEMORY_ID))
  );

  static HEIGHT_TO_STATISTIC_RUNES: RefCell<StableBTreeMap<u32, u64, VMemory>> = RefCell::new(
    StableBTreeMap::init(get_virtual_memory(HEIGHT_TO_STATISTIC_RUNES_MEMORY_ID))
  );

  static HEIGHT_TO_STATISTIC_RESERVED_RUNES: RefCell<StableBTreeMap<u32, u64, VMemory>> = RefCell::new(
    StableBTreeMap::init(get_virtual_memory(HEIGHT_TO_STATISTIC_RESERVED_RUNES_MEMORY_ID))
  );
}


static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);

pub fn shut_down() {
  SHUTTING_DOWN.store(true, atomic::Ordering::Relaxed);
}

pub fn cancel_shutdown() {
  SHUTTING_DOWN.store(false, atomic::Ordering::Relaxed);
}

pub fn is_shutting_down() -> bool {
  SHUTTING_DOWN.load(atomic::Ordering::Relaxed)
}


pub fn mem_get_config() -> Config {
  CONFIG.with(|m| m.borrow().get().clone())
}

pub fn mem_set_config(config: Config) -> Result<Config> {
  CONFIG
    .with(|m| m.borrow_mut().set(config))
    .map_err(|e| anyhow::anyhow!("Failed to set config: {:?}", e))
}


pub fn mem_latest_block() -> Option<(u32, BlockHash)> {
  HEIGHT_TO_BLOCK_HEADER.with(|m| {
    m.borrow()
      .iter()
      .rev()
      .next()
      .map(|(height, header_value)| {
        let header = Header::load(header_value);
        (height, header.block_hash())
      })
  })
}

pub(crate) fn mem_latest_block_height() -> Option<u32> {
  HEIGHT_TO_BLOCK_HEADER.with(|m| m.borrow().iter().rev().next().map(|(height, _)| height))
}

pub(crate) fn mem_block_hash(height: u32) -> Option<BlockHash> {
  HEIGHT_TO_BLOCK_HEADER.with(|m| {
    m.borrow()
      .get(&height)
      .map(|header_value| Header::load(header_value).block_hash())
  })
}

pub(crate) fn mem_insert_block_header(height: u32, header_value: HeaderValue) {
  HEIGHT_TO_BLOCK_HEADER.with(|m| m.borrow_mut().insert(height, header_value));
}

pub(crate) fn mem_remove_block_header(height: u32) -> Option<HeaderValue> {
  HEIGHT_TO_BLOCK_HEADER.with(|m| m.borrow_mut().remove(&height))
}

pub(crate) fn mem_prune_block_header(height: u32) {
  HEIGHT_TO_BLOCK_HEADER.with(|m| {
    let mut map = m.borrow_mut();
    let keys_to_remove: Vec<u32> = map
      .iter()
      .take_while(|(h, _)| *h <= height)
      .map(|(h, _)| h)
      .collect();
    for key in keys_to_remove {
      map.remove(&key);
    }
  });
}

pub(crate) fn next_block(index: &Index) -> (u32, Option<BlockHash>) {
  mem_latest_block()
    .map(|(height, prev_blockhash)| (height + 1, Some(prev_blockhash)))
    .unwrap_or((index.get_first_index_height(), None))
}

pub(crate) fn mem_insert_sat_to_sequence_numbers(sat: u64, seq: u32) -> bool {
  SAT_TO_SEQUENCE_NUMBERS.with(|m| {
    let mut map = m.borrow_mut();
    let mut sequence_numbers = map.get(&sat).unwrap_or_default();
    if sequence_numbers.contains(seq) {
      return false;
    }
    sequence_numbers.push(seq);
    map.insert(sat, sequence_numbers);
    return true;
  })
}

pub(crate) fn mem_get_first_seq_of_sat_to_sequence_numbers(sat: u64) -> Option<u32> {
  SAT_TO_SEQUENCE_NUMBERS.with(|m| m.borrow().get(&sat).map(|seqs| seqs.0[0]))
}

pub(crate) fn mem_insert_sequence_number_to_childrens(seq: u32, seq_children: u32) -> bool {
  SEQUENCE_NUMBER_TO_CHILDRENS.with(|m| {
    let mut map = m.borrow_mut();
    let mut sequence_numbers = map.get(&seq).unwrap_or_default();
    if sequence_numbers.contains(seq_children) {
      return false;
    }
    sequence_numbers.push(seq_children);
    map.insert(seq, sequence_numbers);
    return true;
  })
}

pub(crate) fn mem_get_sequence_number_to_childrens(seq: u32) -> Option<SequenceNumbers> {
  SEQUENCE_NUMBER_TO_CHILDRENS.with(|m| m.borrow().get(&seq))
}

pub(crate) fn mem_insert_script_pubkey_to_outpoints(script_pubkey: Vec<u8>, outpoint: OutPoint) -> bool {
  SCRIPT_PUBKEY_TO_OUTPOINTS.with(|m| {
    let mut map = m.borrow_mut();
    let mut outpoints = map.get(&script_pubkey).unwrap_or_default();
    if outpoints.contains(&outpoint) {
      return false;
    }
    outpoints.push(outpoint);
    map.insert(script_pubkey, outpoints);
    return true;
  })
}

pub(crate) fn mem_remove_script_pubkey_to_outpoints(script_pubkey: Vec<u8>, outpoint: &OutPoint) -> bool {
  SCRIPT_PUBKEY_TO_OUTPOINTS.with(|m| {
    let mut map = m.borrow_mut();
    let mut outpoints = map.get(&script_pubkey).unwrap_or_default();
    if !outpoints.contains(outpoint) {
      return false;
    }
    outpoints.retain(|&o| o != *outpoint);
    map.insert(script_pubkey, outpoints);
    return true;
  })
}



pub(crate) fn mem_get_sequence_number_to_inscription_entry(seq: u32) -> Option<InscriptionEntry> {
  SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY.with(|m| m.borrow().get(&seq))
}

pub(crate) fn mem_insert_sequence_number_to_inscription_entry(seq: u32, entry: InscriptionEntry) {
  SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY.with(|m| m.borrow_mut().insert(seq, entry));
}

pub(crate) fn mem_get_next_sequence_of_sequence_number_to_inscription_entry()-> u32 {
  SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY.with(|m| m.borrow().iter().next_back().map(|(seq, _)| seq + 1).unwrap_or(0))
}

pub(crate) fn mem_get_inscription_id_to_sequence_number(id: &InscriptionId) -> Option<u32> {
  INSCRIPTION_ID_TO_SEQUENCE_NUMBER.with(|m| m.borrow().get(id))
}

pub(crate) fn mem_insert_inscription_id_to_sequence_number(id: InscriptionId, seq: u32) {
  INSCRIPTION_ID_TO_SEQUENCE_NUMBER.with(|m| m.borrow_mut().insert(id, seq));
}

pub(crate) fn mem_insert_transaction_id_to_transaction(id: TxidValue, tx: Vec<u8>) {
  TRANSACTION_ID_TO_TRANSACTION.with(|m| m.borrow_mut().insert(id, tx));
}

pub(crate) fn mem_get_transaction_id_to_transaction(id: TxidValue) -> Option<Vec<u8>> {
  TRANSACTION_ID_TO_TRANSACTION.with(|m| m.borrow().get(&id))
}

pub(crate) fn mem_insert_inscription_number_to_sequence_number(num: &InscriptionNumber, seq: u32) {
  INSCRIPTION_NUMBER_TO_SEQUENCE_NUMBER.with(|m| m.borrow_mut().insert(num.clone(), seq));
}

pub(crate) fn mem_get_inscription_number_to_sequence_number(num: &InscriptionNumber) -> Option<u32> {
  INSCRIPTION_NUMBER_TO_SEQUENCE_NUMBER.with(|m| m.borrow().get(num))
}

pub(crate) fn mem_insert_home_inscriptions(seq: u32, id: InscriptionId) {
  HOME_INSCRIPTIONS.with(|m| m.borrow_mut().insert(seq, id));
}

pub(crate) fn mem_pop_first_home_inscriptions() -> Option<(u32,InscriptionId)> {
  HOME_INSCRIPTIONS.with(|m| m.borrow_mut().pop_first())
}

pub(crate) fn mem_get_home_inscriptions_len() -> u64 {
  HOME_INSCRIPTIONS.with(|m| m.borrow().len())
}

// pub(crate) fn mem_insert_height_to_events(height: u32, events: Events) {
//   HEIGHT_TO_EVENTS.with(|m| m.borrow_mut().insert(height, events));
// }

// pub(crate) fn mem_remove_height_to_events(height: u32) {
//   HEIGHT_TO_EVENTS.with(|m| m.borrow_mut().remove(&height));
// }

pub(crate) fn mem_insert_statistic_to_count(statistic: Statistic, count: u64) {
  STATISTIC_TO_COUNT.with(|m| m.borrow_mut().insert(statistic.key(), count));
}

pub(crate) fn mem_increment_statistic(statistic: Statistic, n:u64) {
  STATISTIC_TO_COUNT.with(|m| {
    let mut map = m.borrow_mut();
    let count = map.get(&statistic.key()).unwrap_or_default();
    map.insert(statistic.key(), count + n);
  });
}

pub(crate) fn mem_get_statistic_count(statistic: Statistic) -> u64 {
  STATISTIC_TO_COUNT.with(|m| m.borrow().get(&statistic.key()).unwrap_or_default())
}

pub(crate) fn mem_remove_outpoint_to_utxo_entry(outpoint: OutPointValue) -> Option<UtxoEntry> {
  OUTPOINT_TO_UTXO_ENTRY.with(|m| m.borrow_mut().remove(&outpoint))
}

pub(crate) fn mem_get_outpoint_to_utxo_entry(outpoint: OutPointValue) -> Option<UtxoEntry> {
  OUTPOINT_TO_UTXO_ENTRY.with(|m| m.borrow().get(&outpoint))
}

pub(crate) fn mem_insert_outpoint_to_utxo_entry(outpoint: OutPointValue, utxo_entry: UtxoEntry) {
  OUTPOINT_TO_UTXO_ENTRY.with(|m| m.borrow_mut().insert(outpoint, utxo_entry));
}

pub(crate) fn mem_insert_sat_to_satpoint(sat: u64, satpoint: SatPointValue) {
  SAT_TO_SATPOINT.with(|m| m.borrow_mut().insert(sat, satpoint));
}

pub(crate) fn mem_insert_height_to_last_sequence_number(height: u32, seq: u32) {
  HEIGHT_TO_LAST_SEQUENCE_NUMBER.with(|m| m.borrow_mut().insert(height, seq));
}

pub(crate) fn mem_get_height_to_last_sequence_number(height: u32) -> Option<u32> {
  HEIGHT_TO_LAST_SEQUENCE_NUMBER.with(|m| m.borrow().get(&height))
}

pub(crate) fn mem_insert_sequence_number_to_satpoint(seq: u32, satpoint: SatPointValue) {
  SEQUENCE_NUMBER_TO_SATPOINT.with(|m| m.borrow_mut().insert(seq, satpoint));
}

pub(crate) fn mem_get_sequence_number_to_satpoint(seq: u32) -> Option<SatPointValue> {
  SEQUENCE_NUMBER_TO_SATPOINT.with(|m| m.borrow().get(&seq))
}

pub(crate) fn mem_get_sequence_number_to_rune_id(seq: u32) -> Option<RuneIdValue> {
  SEQUENCE_NUMBER_TO_RUNE_ID.with(|m| m.borrow().get(&seq))
}

pub(crate) fn mem_length_outpoint_to_rune_balances() -> u64 {
  OUTPOINT_TO_RUNE_BALANCES.with(|m| m.borrow().len())
}

pub(crate) fn mem_get_outpoint_to_rune_balances(outpoint_value: OutPointValue) -> Option<RuneBalances> {
  OUTPOINT_TO_RUNE_BALANCES.with(|m| m.borrow().get(&outpoint_value))
}

pub(crate) fn mem_insert_outpoint_to_rune_balances(
  outpoint_value: OutPointValue,
  rune_balances: RuneBalances,
) {
  OUTPOINT_TO_RUNE_BALANCES.with(|m| m.borrow_mut().insert(outpoint_value, rune_balances));
}

pub(crate) fn mem_remove_outpoint_to_rune_balances(
  outpoint_value: OutPointValue,
) -> Option<RuneBalances> {
  OUTPOINT_TO_RUNE_BALANCES.with(|m| m.borrow_mut().remove(&outpoint_value))
}

pub(crate) fn mem_length_outpoint_to_height() -> u64 {
  OUTPOINT_TO_HEIGHT.with(|m| m.borrow().len())
}

pub(crate) fn mem_get_outpoint_to_height(outpoint: OutPointValue) -> Option<u32> {
  OUTPOINT_TO_HEIGHT.with(|m| m.borrow().get(&outpoint))
}

pub(crate) fn mem_insert_outpoint_to_height(outpoint: OutPointValue, height: u32) {
  OUTPOINT_TO_HEIGHT.with(|m| m.borrow_mut().insert(outpoint, height));
}

pub(crate) fn mem_remove_outpoint_to_height(outpoint_value: OutPointValue) -> Option<u32> {
  OUTPOINT_TO_HEIGHT.with(|m| m.borrow_mut().remove(&outpoint_value))
}

// pub fn mem_length_change_record_rune() -> u64 {
//   HEIGHT_TO_CHANGE_RECORD_RUNE.with(|m| m.borrow().len())
// }

pub(crate) fn mem_insert_change_record_rune(height: u32, change_record: ChangeRecordRune) {
  HEIGHT_TO_CHANGE_RECORD_RUNE.with(|m| m.borrow_mut().insert(height, change_record));
}

pub(crate) fn mem_get_change_record_rune(height: u32) -> Option<ChangeRecordRune> {
  HEIGHT_TO_CHANGE_RECORD_RUNE.with(|m| m.borrow().get(&height))
}

pub(crate) fn mem_remove_change_record_rune(height: u32) -> Option<ChangeRecordRune> {
  HEIGHT_TO_CHANGE_RECORD_RUNE.with(|m| m.borrow_mut().remove(&height))
}

pub fn mem_prune_change_record_rune(height: u32) {
  HEIGHT_TO_CHANGE_RECORD_RUNE.with(|m| {
    let mut map = m.borrow_mut();
    let keys_to_remove: Vec<u32> = map
      .iter()
      .take_while(|(h, _)| *h <= height)
      .map(|(h, _)| h)
      .collect();
    for key in keys_to_remove {
      map.remove(&key);
    }
  });
}

pub fn mem_length_rune_id_to_rune_entry() -> u64 {
  RUNE_ID_TO_RUNE_ENTRY.with(|m| m.borrow().len())
}

pub fn mem_get_rune_id_to_rune_entry(rune_id_value: RuneIdValue) -> Option<RuneEntry> {
  RUNE_ID_TO_RUNE_ENTRY.with(|m| m.borrow().get(&rune_id_value))
}

pub fn mem_insert_rune_id_to_rune_entry(rune_id_value: RuneIdValue, rune_entry: RuneEntry) {
  RUNE_ID_TO_RUNE_ENTRY.with(|m| m.borrow_mut().insert(rune_id_value, rune_entry));
}

pub(crate) fn mem_remove_rune_id_to_rune_entry(rune_id_value: RuneIdValue) -> Option<RuneEntry> {
  RUNE_ID_TO_RUNE_ENTRY.with(|m| m.borrow_mut().remove(&rune_id_value))
}

pub fn mem_length_rune_to_rune_id() -> u64 {
  RUNE_TO_RUNE_ID.with(|m| m.borrow().len())
}

pub fn mem_get_rune_to_rune_id(rune: u128) -> Option<RuneIdValue> {
  RUNE_TO_RUNE_ID.with(|m| m.borrow().get(&rune))
}

pub fn mem_insert_rune_to_rune_id(rune: u128, rune_id_value: RuneIdValue) {
  RUNE_TO_RUNE_ID.with(|m| m.borrow_mut().insert(rune, rune_id_value));
}

pub(crate) fn mem_remove_rune_to_rune_id(rune: u128) -> Option<RuneIdValue> {
  RUNE_TO_RUNE_ID.with(|m| m.borrow_mut().remove(&rune))
}

pub fn mem_length_transaction_id_to_rune() -> u64 {
  TRANSACTION_ID_TO_RUNE.with(|m| m.borrow().len())
}

pub fn mem_insert_transaction_id_to_rune(txid: TxidValue, rune: u128) {
  TRANSACTION_ID_TO_RUNE.with(|m| m.borrow_mut().insert(txid, rune));
}

pub(crate) fn mem_remove_transaction_id_to_rune(txid: TxidValue) -> Option<u128> {
  TRANSACTION_ID_TO_RUNE.with(|m| m.borrow_mut().remove(&txid))
}

pub fn mem_statistic_runes() -> u64 {
  HEIGHT_TO_STATISTIC_RUNES.with(|m| {
    m.borrow()
      .iter()
      .rev()
      .next()
      .map(|(_, runes)| runes)
      .unwrap_or(0)
  })
}

pub fn mem_insert_statistic_runes(height: u32, runes: u64) {
  HEIGHT_TO_STATISTIC_RUNES.with(|m| m.borrow_mut().insert(height, runes));
}

pub fn mem_remove_statistic_runes(height: u32) -> Option<u64> {
  HEIGHT_TO_STATISTIC_RUNES.with(|m| m.borrow_mut().remove(&height))
}

pub fn mem_prune_statistic_runes(height: u32) {
  HEIGHT_TO_STATISTIC_RUNES.with(|m| {
    let mut map = m.borrow_mut();
    // Get all keys less or equal than height
    let keys_to_remove: Vec<u32> = map
      .iter()
      .take_while(|(h, _)| *h <= height)
      .map(|(h, _)| h)
      .collect();

    // Remove all entries with those keys
    for key in keys_to_remove {
      map.remove(&key);
    }

    map.remove(&height)
  });
}

pub fn mem_statistic_reserved_runes() -> u64 {
  HEIGHT_TO_STATISTIC_RESERVED_RUNES.with(|m| {
    m.borrow()
      .iter()
      .rev()
      .next()
      .map(|(_, runes)| runes)
      .unwrap_or(0)
  })
}

pub fn mem_insert_statistic_reserved_runes(height: u32, runes: u64) {
  HEIGHT_TO_STATISTIC_RESERVED_RUNES.with(|m| m.borrow_mut().insert(height, runes));
}

pub fn mem_remove_statistic_reserved_runes(height: u32) -> Option<u64> {
  HEIGHT_TO_STATISTIC_RESERVED_RUNES.with(|m| m.borrow_mut().remove(&height))
}

pub fn mem_prune_statistic_reserved_runes(height: u32) {
  HEIGHT_TO_STATISTIC_RESERVED_RUNES.with(|m| {
    let mut map = m.borrow_mut();
    let keys_to_remove: Vec<u32> = map
      .iter()
      .take_while(|(h, _)| *h <= height)
      .map(|(h, _)| h)
      .collect();
    for key in keys_to_remove {
      map.remove(&key);
    }
  });
}

/// helper function
pub(crate) async fn inscription_info(
  index: &Index,
  query: InscriptionQuery,
  child: Option<usize>,
) -> Result<Option<(InscriptionResp, Option<TxOut>, Inscription)>> {
  let sequence_number = match query {
    InscriptionQuery::Id(id) => mem_get_inscription_id_to_sequence_number(&id),
    InscriptionQuery::Number(inscription_number) => mem_get_inscription_number_to_sequence_number(&InscriptionNumber::from(inscription_number)),
    InscriptionQuery::Sat(sat) => mem_get_first_seq_of_sat_to_sequence_numbers(sat.n()),
  };

  let Some(sequence_number) = sequence_number else {
    return Ok(None);
  };

  let sequence_number = if let Some(child) = child {
    let Some(child) = mem_get_sequence_number_to_childrens(sequence_number).iter().nth(child).map(|seqs| seqs.0[0])
    else {
      return Ok(None);
    };

    child
  } else {
    sequence_number
  };

  let entry = mem_get_sequence_number_to_inscription_entry(sequence_number).unwrap();

  let Some(transaction) = get_transaction(index,entry.id.txid).await? else {
    return Ok(None);
  };

  let Some(inscription) = ParsedEnvelope::from_transaction(&transaction)
    .into_iter()
    .nth(entry.id.index as usize)
    .map(|envelope| envelope.payload)
  else {
    return Ok(None);
  };

  let satpoint = SatPoint::load(
    mem_get_sequence_number_to_satpoint(sequence_number)
      .unwrap()
  );

  let output = if satpoint.outpoint == unbound_outpoint() || satpoint.outpoint == OutPoint::null()
  {
    None
  } else {
    let Some(transaction) = get_transaction(index, satpoint.outpoint.txid).await? else {
      return Ok(None);
    };

    transaction
      .output
      .into_iter()
      .nth(satpoint.outpoint.vout.try_into().unwrap())
  };

  let previous = if let Some(n) = sequence_number.checked_sub(1) {
    Some(
      mem_get_sequence_number_to_inscription_entry(n)
        .unwrap()
        .id,
    )
  } else {
    None
  };

  let next = mem_get_sequence_number_to_inscription_entry(sequence_number + 1)
    .map(|entry| entry.id);

  let all_children = mem_get_sequence_number_to_childrens(sequence_number).unwrap_or_default();

  let child_count = all_children.len() as u64;

  let children = all_children.0.iter()
    .take(4)
    .map(|sequence_number| {
      let entry = mem_get_sequence_number_to_inscription_entry(*sequence_number).unwrap();
      Ok(entry.id)
    })
    .collect::<Result<Vec<InscriptionId>>>()?;

  let rune = if let Some(rune_id) = mem_get_sequence_number_to_rune_id(sequence_number)
  {
    let entry = mem_get_rune_id_to_rune_entry(rune_id).unwrap();
    Some(entry.spaced_rune)
  } else {
    None
  };

  let parents = entry
    .parents
    .iter()
    .take(4)
    .map(|parent| {
      Ok(
        mem_get_sequence_number_to_inscription_entry(*parent)
          .unwrap()
          .id,
      )
    })
    .collect::<Result<Vec<InscriptionId>>>()?;

  let mut charms = entry.charms;

  if satpoint.outpoint == OutPoint::null() {
    Charm::Lost.set(&mut charms);
  }

  let effective_mime_type = if let Some(delegate_id) = inscription.delegate() {
    let delegate_result = get_inscription_by_id(index, delegate_id).await;
    if let Ok(Some(delegate)) = delegate_result {
      delegate.content_type().map(str::to_string)
    } else {
      inscription.content_type().map(str::to_string)
    }
  } else {
    inscription.content_type().map(str::to_string)
  };

  Ok(Some((
    InscriptionResp {
      address: output
        .as_ref()
        .and_then(|o| {
          index
            .chain()
            .address_from_script(&o.script_pubkey)
            .ok()
        })
        .map(|address| address.to_string()),
      charms: Charm::charms(charms),
      child_count,
      children,
      content_length: inscription.content_length(),
      content_type: inscription.content_type().map(|s| s.to_string()),
      effective_content_type: effective_mime_type,
      fee: entry.fee,
      height: entry.height,
      id: entry.id,
      next,
      number: entry.inscription_number,
      parents,
      previous,
      rune,
      sat: entry.sat,
      satpoint,
      timestamp: timestamp(entry.timestamp.into()).timestamp(),
      value: output.as_ref().map(|o| o.value.to_sat()),
      metaprotocol: inscription.metaprotocol().map(|s| s.to_string()),
    },
    output,
    inscription,
  )))
}

pub fn get_inscriptions_for_output(
  index: &Index,
  outpoint: OutPoint,
) -> Result<Option<Vec<InscriptionId>>> {
  let Some(inscriptions) = inscriptions_on_output(index,outpoint)? else {
    return Ok(None);
  };

  Ok(Some(
    inscriptions
      .iter()
      .map(|(_satpoint, inscription_id)| *inscription_id)
      .collect(),
  ))
}

pub fn inscriptions_on_output(
  index: &Index,
  outpoint: OutPoint,
) -> Result<Option<Vec<(SatPoint, InscriptionId)>>> {
  if !index.index_inscriptions {
    return Ok(None);
  }

  let Some(utxo_entry) = mem_get_outpoint_to_utxo_entry(outpoint.store()) else {
    return Ok(Some(Vec::new()));
  };

  let mut inscriptions = utxo_entry.parse(index).parse_inscriptions();

  inscriptions.sort_by_key(|(sequence_number, _)| *sequence_number);

  inscriptions
    .into_iter()
    .map(|(sequence_number, offset)| {
      let entry = mem_get_sequence_number_to_inscription_entry(sequence_number)
        .unwrap();

      let satpoint = SatPoint { outpoint, offset };

      Ok((satpoint,entry.id))
    })
    .collect::<Result<_>>()
    .map(Some)
}

pub async fn get_transaction(index: &Index, txid: Txid) -> Result<Option<Transaction>> {
  if txid == index.genesis_block_coinbase_txid {
    return Ok(Some(index.genesis_block_coinbase_transaction.clone()));
  }

  if index.index_transactions {
    if let Some(transaction) = mem_get_transaction_id_to_transaction(txid.store())
    {
      return Ok(Some(consensus::encode::deserialize(transaction.as_ref())?));
    }
  }
  // if considered a ddos attack, we can return None here
  let tx = crate::rpc::get_raw_transaction_info(&txid, None).await?.transaction()?;
  Ok(Some(tx))
}

pub fn inscription_exists(inscription_id: InscriptionId) -> Result<bool> {
  Ok(
    mem_get_inscription_id_to_sequence_number(&inscription_id)
      .is_some(),
  )
}

pub async fn get_inscription_by_id(
  index: &Index,
  inscription_id: InscriptionId,
) -> Result<Option<Inscription>> {
  if !inscription_exists(inscription_id)? {
    return Ok(None);
  }

  let Some(transaction) = get_transaction(index, inscription_id.txid).await? else {
    return Ok(None);
  };

  Ok(Some(
    ParsedEnvelope::from_transaction(&transaction)
      .into_iter()
      .nth(inscription_id.index as usize)
      .map(|envelope| envelope.payload)
      .unwrap(),
  ))
}


pub fn get_inscriptions_in_block(block_height: u32) -> Result<Vec<InscriptionId>> {
  let Some(newest_sequence_number) = mem_get_height_to_last_sequence_number(block_height)
  else {
    return Ok(Vec::new());
  };

  let oldest_sequence_number = mem_get_height_to_last_sequence_number(block_height.saturating_sub(1))
    .unwrap_or(0);

  (oldest_sequence_number..newest_sequence_number)
    .map(|num| match mem_get_sequence_number_to_inscription_entry(num) {
      Some(inscription_entry) => Ok(inscription_entry.id),
      None => Err(anyhow!(
        "could not find inscription for inscription number {num}"
      )),
    })
    .collect::<Result<Vec<InscriptionId>>>()
}

pub fn mem_get_etching(txid: Txid) -> Option<(RuneId, RuneEntry)> {
  TRANSACTION_ID_TO_RUNE.with(|m| {
    m.borrow()
      .get(&Txid::store(txid))
      .and_then(|rune| RUNE_TO_RUNE_ID.with(|m| m.borrow().get(&rune)))
      .and_then(|id| {
        RUNE_ID_TO_RUNE_ENTRY.with(|m| m.borrow().get(&id).map(|e| (RuneId::load(id), e)))
      })
  })
}
