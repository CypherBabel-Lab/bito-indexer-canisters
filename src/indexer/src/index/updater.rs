use std::collections::HashMap;

use bitcoin::{block::Header, Block, OutPoint, Transaction, Txid};
use bitcoincore_rpc_json::GetRawTransactionResult;
use candid::Principal;
use ic_canister_log::log;
use ic_cdk::api::management_canister::bitcoin::BitcoinNetwork;
use inscription_updater::InscriptionUpdater;
use logs::{ERROR, INFO};
use ordinals::{Height, Rune, Sat, SatPoint};
use rune_updater::RuneUpdater;
use crate::{index::{entry::{ChangeRecordRune, Entry, SatRange}, event::Events, mem_get_home_inscriptions_len, mem_get_next_sequence_of_sequence_number_to_inscription_entry, mem_get_outpoint_to_utxo_entry, mem_get_statistic_count, mem_increment_statistic, mem_insert_height_to_last_sequence_number, mem_insert_outpoint_to_utxo_entry, mem_insert_script_pubkey_to_outpoints, mem_insert_sequence_number_to_satpoint, mem_insert_statistic_to_count, mem_latest_block, mem_length_outpoint_to_height, mem_length_outpoint_to_rune_balances, mem_length_rune_id_to_rune_entry, mem_length_rune_to_rune_id, mem_length_transaction_id_to_rune, mem_remove_outpoint_to_utxo_entry, mem_remove_script_pubkey_to_outpoints, mem_statistic_reserved_runes, mem_statistic_runes, utxo_entry::{ParsedUtxoEntry, UtxoEntryBuf}, Statistic}, timestamp, Result};

use super::{is_shutting_down, mem_insert_block_header, mem_insert_sat_to_satpoint, next_block, reorg::{self, Reorg}, Index};

mod inscription_updater;
mod rune_updater;

pub(crate) struct BlockData {
    pub(crate) header: Header,
    pub(crate) txdata: Vec<(Transaction, Txid)>,
  }
  
impl From<Block> for BlockData {
    fn from(block: Block) -> Self {
        BlockData {
        header: block.header,
        txdata: block
            .txdata
            .into_iter()
            .map(|transaction| {
            let txid = transaction.compute_txid();
            (transaction, txid)
            })
            .collect(),
        }
    }
}

pub(crate) struct Updater {
    pub(super) height: u32,
    pub(super) outputs_cached: u64,
    pub(super) outputs_traversed: u64,
    pub(super) sat_ranges_since_flush: u64,
    pub(super) outputs_in_stable_memory: u64,
    pub(super) outputs_https_outcall: u64,
}

pub fn update_index(network: BitcoinNetwork, index: Index, subscribers: Vec<Principal>) -> Result {
    ic_cdk_timers::set_timer(std::time::Duration::from_secs(10), move || {
      ic_cdk::spawn(async move {
        let (height, index_prev_blockhash) = next_block(&index);
        match crate::bitcoin_api::get_block_hash(network, height).await {
          Ok(Some(block_hash)) => match crate::rpc::get_block(block_hash).await {
            Ok(block) => {
              match Reorg::detect_reorg(
                network,
                index_prev_blockhash,
                &block,
                height,
              )
              .await
              {
                Ok(()) => {
                  let txids: Vec<String> = block
                    .txdata
                    .iter()
                    .map(|(_, txid)| txid.to_string())
                    .collect();
                  if let Err(e) = index_block(height, &block, &index).await {
                    log!(
                      ERROR,
                      "failed to index_block at height {}: {:?}",
                      height,
                      e
                    );
                  } else {
                    Reorg::prune_change_record(height);
                    mem_insert_block_header(height, block.header.store());
                    log!(
                      INFO,
                      "indexed block_height: {} block_hash: {}",
                      height,
                      block_hash.to_string()
                    );
                    for subscriber in subscribers.iter() {
                      let _ = crate::notifier::notify_new_block(
                        *subscriber,
                        height,
                        block_hash.to_string(),
                        txids.clone(),
                      )
                      .await;
                      log!(
                        INFO,
                        "notified subscriber: {:?} with block_height: {:?} block_hash: {:?}",
                        subscriber,
                        height,
                        block_hash
                      );
                    }
                  }
                }
                Err(e) => match e {
                  reorg::Error::Recoverable { height, depth } => {
                    Reorg::handle_reorg(height, depth, &index);
                  }
                  reorg::Error::Unrecoverable => {
                    log!(
                      ERROR,
                      "unrecoverable reorg detected at height {}",
                      height
                    );
                    return;
                  }
                },
              }
            }
            Err(e) => {
              log!(
                ERROR,
                "failed to get_block: {:?} error: {:?}",
                block_hash,
                e
              );
            }
          },
          Ok(None) => {}
          Err(e) => {
            log!(
              ERROR,
              "failed to get_block_hash at height {}: {:?}",
              height,
              e
            );
            return;
          }
        }
        if is_shutting_down() {
          log!(
            INFO,
            "shutting down index thread, skipping update at height {}",
            height
          );
        } else {
          let _ = update_index(network, index, subscribers);
        }
      });
    });
  
    Ok(())
  }

  async fn index_block(height: u32, block: &BlockData, index: &Index) -> Result {
    log!(
      INFO,
      "Block {} at {} with {} transactions…",
      height,
      timestamp(block.header.time.into()),
      block.txdata.len()
    );
    if index.index_inscriptions ||index.index_addresses || index.index_sats {
      index_utxo_entries(height, block, index).await?;
    }
    if index.index_runes && height >= index.first_rune_height() {
      index_rune(height, block).await?;
    }
    Ok(())
  }

  async fn index_rune(height: u32, block: &BlockData) -> Result {
    let runes = mem_statistic_runes();
    let reserved_runes = mem_statistic_reserved_runes();
  
    if height % 10 == 0 {
      log!(
        INFO,
        "Index statistics at height {}: latest_block: {:?}, reserved_runes: {}, runes: {}, rune_to_rune_id: {}, rune_entry: {}, transaction_id_to_rune: {}, outpoint_to_rune_balances: {}, outpoint_to_height: {}",
        height,
        mem_latest_block(),
        reserved_runes,
        runes,
        mem_length_rune_to_rune_id(),
        mem_length_rune_id_to_rune_entry(),
        mem_length_transaction_id_to_rune(),
        mem_length_outpoint_to_rune_balances(),
        mem_length_outpoint_to_height(),
      );
    }
  
    // init statistic runes/reserved_runes for new height
    crate::index::mem_insert_statistic_runes(height, runes);
    crate::index::mem_insert_statistic_reserved_runes(height, reserved_runes);
  
    let mut rune_updater = RuneUpdater {
      block_time: block.header.time,
      burned: HashMap::new(),
      height,
      minimum: Rune::minimum_at_height(bitcoin::Network::Bitcoin, Height(height)),
      runes,
      change_record: ChangeRecordRune::new(),
      events: Events::new(),
    };
  
    for (i, (tx, txid)) in block.txdata.iter().enumerate() {
      rune_updater
        .index_runes(u32::try_from(i).unwrap(), tx, *txid)
        .await?;
    }
  
    rune_updater.update()?;
  
    Ok(())
  }


  async fn index_utxo_entries(height: u32, block: &BlockData, index: &Index) -> Result {
    let mut sat_ranges_written = 0;
    let mut outputs_in_block = 0;
    let index_inscriptions = height >= index.first_inscription_height() && index.index_inscriptions;
    let mut lost_sats = mem_get_statistic_count(crate::index::Statistic::LostSats);
    let cursed_inscription_count = mem_get_statistic_count(crate::index::Statistic::CursedInscriptions);
    let blessed_inscription_count = mem_get_statistic_count(crate::index::Statistic::BlessedInscriptions);
    let unbound_inscriptions = mem_get_statistic_count(crate::index::Statistic::UnboundInscriptions);
    let next_sequence_number = mem_get_next_sequence_of_sequence_number_to_inscription_entry();
    let home_inscription_count = mem_get_home_inscriptions_len();
    let mut inscription_updater = InscriptionUpdater{
        blessed_inscription_count,
        cursed_inscription_count,
        flotsam: Vec::new(),
        height,
        home_inscription_count,
        lost_sats,
        next_sequence_number,
        reward: Height(height).subsidy(),
        transaction_buffer: Vec::new(),
        timestamp: block.header.time,
        unbound_inscriptions,
        events: Events::new(),
    };

    let mut coinbase_inputs = Vec::new();
    let mut lost_sat_ranges = Vec::new();

    let mut updater = Updater {
        height,
        outputs_cached: 0,
        outputs_traversed: 0,
        sat_ranges_since_flush: 0,
        outputs_in_stable_memory: 0,
        outputs_https_outcall: 0,
    };

    let mut utxo_cache = HashMap::new();

    if index.index_sats {
        let h = Height(height);
        if h.subsidy() > 0 {
          let start = h.starting_sat();
          coinbase_inputs.extend(SatRange::store((start.n(), (start + h.subsidy()).n())));
          updater.sat_ranges_since_flush += 1;
        }
    }

    for (tx_offset, (tx, txid)) in block
        .txdata
        .iter()
        .enumerate()
        .skip(1)
        .chain(block.txdata.iter().enumerate().take(1))
    {
      log!(INFO,"Indexing block on height({height})'s transaction {tx_offset}-{txid}…");

      let mut input_utxo_entries: Vec<UtxoEntryBuf> = Vec::new();
      if tx_offset != 0 {
          for input in tx.input.iter() {
              let outpoint = input.previous_output;
              let entry = if let Some(entry) = utxo_cache.remove(&outpoint) {
                  updater.outputs_cached += 1;
                  entry
              } else if let Some(entry) = mem_remove_outpoint_to_utxo_entry(outpoint.store()) {
                  if index.index_addresses {
                      let script_pubkey = entry.parse(index).script_pubkey();
                      if !mem_remove_script_pubkey_to_outpoints(script_pubkey.to_vec(), &outpoint) {
                          panic!("script pubkey entry ({script_pubkey:?}, {outpoint:?}) not found");
                      }
                  }
                  updater.outputs_in_stable_memory += 1;
                  entry.to_buf()
              } else {
                  assert!(!index.have_full_utxo_index());
                  let tx_info = get_raw_transaction_info_forever(outpoint.txid).await?;
                  let txout = tx_info.transaction()?.tx_out(outpoint.vout as usize)?.clone();

                  let mut entry = UtxoEntryBuf::new();
                  entry.push_value(txout.value.to_sat(), index);
                  if index.index_addresses {
                      entry.push_script_pubkey(txout.script_pubkey.as_bytes(), index);
                  }
                  updater.outputs_https_outcall += 1;
                  entry
              };
              input_utxo_entries.push(entry);
          }
        } 
      let input_utxo_entries = input_utxo_entries
          .iter()
          .map(|entry| entry.parse(index))
          .collect::<Vec<ParsedUtxoEntry>>();

      let mut output_utxo_entries = tx
        .output
        .iter()
        .map(|_| UtxoEntryBuf::new())
        .collect::<Vec<UtxoEntryBuf>>();

      let input_sat_ranges;
      if index.index_sats {
        let leftover_sat_ranges;

        if tx_offset == 0 {
          input_sat_ranges = Some(vec![coinbase_inputs.as_slice()]);
          leftover_sat_ranges = &mut lost_sat_ranges;
        } else {
          input_sat_ranges = Some(
            input_utxo_entries
              .iter()
              .map(|entry| entry.sat_ranges())
              .collect(),
          );
          leftover_sat_ranges = &mut coinbase_inputs;
        }

        index_transaction_sats(
          &mut updater,
          index,
          tx,
          *txid,
          &mut output_utxo_entries,
          input_sat_ranges.as_ref().unwrap(),
          leftover_sat_ranges,
          &mut sat_ranges_written,
          &mut outputs_in_block,
        )?;
      } else {
        input_sat_ranges = None;

        for (vout, txout) in tx.output.iter().enumerate() {
          output_utxo_entries[vout].push_value(txout.value.to_sat(), index);
        }
      }

      if index.index_addresses {
        index_transaction_output_script_pubkeys(index,tx, &mut output_utxo_entries);
      }

      if index_inscriptions {
        inscription_updater.index_inscriptions(
          tx,
          *txid,
          &input_utxo_entries,
          &mut output_utxo_entries,
          &mut utxo_cache,
          index,
          input_sat_ranges.as_ref(),
        )?;
      }

      for (vout, output_utxo_entry) in output_utxo_entries.into_iter().enumerate() {
        let vout = u32::try_from(vout).unwrap();
        utxo_cache.insert(OutPoint { txid: *txid, vout }, output_utxo_entry);
      }
    }

    if index_inscriptions {
      mem_insert_height_to_last_sequence_number(height, inscription_updater.next_sequence_number);
    }

    if !lost_sat_ranges.is_empty() {
      // Note that the lost-sats outpoint is special, because (unlike real
      // outputs) it gets written to more than once.  commit() will merge
      // our new entry with any existing one.
      let utxo_entry = utxo_cache
        .entry(OutPoint::null())
        .or_insert(UtxoEntryBuf::empty(index));

      for chunk in lost_sat_ranges.chunks_exact(11) {
        let (start, end) = SatRange::load(chunk.try_into().unwrap());
        if !Sat(start).common() {
          mem_insert_sat_to_satpoint(start, SatPoint {
            outpoint: OutPoint::null(),
            offset: lost_sats,
          }
          .store());
        }

        lost_sats += end - start;
      }

      let mut new_utxo_entry = UtxoEntryBuf::new();
      new_utxo_entry.push_sat_ranges(&lost_sat_ranges, index);
      if index.index_addresses {
        new_utxo_entry.push_script_pubkey(&[], index);
      }

      *utxo_entry = UtxoEntryBuf::merged(utxo_entry, &new_utxo_entry, index);
      mem_insert_statistic_to_count(crate::index::Statistic::LostSats, inscription_updater.lost_sats);
      mem_insert_statistic_to_count(crate::index::Statistic::CursedInscriptions, inscription_updater.cursed_inscription_count);
      mem_insert_statistic_to_count(crate::index::Statistic::BlessedInscriptions, inscription_updater.blessed_inscription_count);
      mem_insert_statistic_to_count(crate::index::Statistic::UnboundInscriptions, inscription_updater.unbound_inscriptions);
    }
    commit(&updater, index, utxo_cache)?;
    Ok(())
  }

  fn index_transaction_output_script_pubkeys(
    index: &Index,
    tx: &Transaction,
    output_utxo_entries: &mut [UtxoEntryBuf],
  ) {
    for (vout, txout) in tx.output.iter().enumerate() {
      output_utxo_entries[vout].push_script_pubkey(txout.script_pubkey.as_bytes(), index);
    }
    
  }

  fn index_transaction_sats(
    updater: &mut Updater,
    index: &Index,
    tx: &Transaction,
    txid: Txid,
    output_utxo_entries: &mut [UtxoEntryBuf],
    input_sat_ranges: &[&[u8]],
    leftover_sat_ranges: &mut Vec<u8>,
    sat_ranges_written: &mut u64,
    outputs_traversed: &mut u64,
  ) -> Result {
    let mut pending_input_sat_range = None;
    let mut input_sat_ranges_iter = input_sat_ranges
      .iter()
      .flat_map(|slice| slice.chunks_exact(11));

    // Preallocate our temporary array, sized to hold the combined
    // sat ranges from our inputs.  We'll never need more than that
    // for a single output, even if we end up splitting some ranges.
    let mut sats = Vec::with_capacity(
      input_sat_ranges
        .iter()
        .map(|slice| slice.len())
        .sum::<usize>(),
    );

    for (vout, output) in tx.output.iter().enumerate() {
      let outpoint = OutPoint {
        vout: vout.try_into().unwrap(),
        txid,
      };

      let mut remaining = output.value.to_sat();
      while remaining > 0 {
        let range = pending_input_sat_range.take().unwrap_or_else(|| {
          SatRange::load(
            input_sat_ranges_iter
              .next()
              .expect("insufficient inputs for transaction outputs")
              .try_into()
              .unwrap(),
          )
        });

        if !Sat(range.0).common() {
          mem_insert_sat_to_satpoint(range.0, SatPoint {
            outpoint,
            offset: output.value.to_sat() - remaining,
          }
          .store());
        }

        let count = range.1 - range.0;

        let assigned = if count > remaining {
          updater.sat_ranges_since_flush += 1;
          let middle = range.0 + remaining;
          pending_input_sat_range = Some((middle, range.1));
          (range.0, middle)
        } else {
          range
        };

        sats.extend_from_slice(&assigned.store());

        remaining -= assigned.1 - assigned.0;

        *sat_ranges_written += 1;
      }

      *outputs_traversed += 1;

      output_utxo_entries[vout].push_sat_ranges(&sats, index);
      sats.clear();
    }

    if let Some(range) = pending_input_sat_range {
      leftover_sat_ranges.extend(&range.store());
    }
    leftover_sat_ranges.extend(input_sat_ranges_iter.flatten());

    Ok(())
  }

  fn commit(
    updater: &Updater,
    index: &Index,
    utxo_cache: HashMap<OutPoint, UtxoEntryBuf>,
  ) -> Result {
    log!(INFO,
      "Committing at block height {}, {} outputs traversed, {} in map, {} cached, {} in stable memory, {} HTTPS outcalls",
      updater.height,
      updater.outputs_traversed,
      utxo_cache.len(),
      updater.outputs_cached,
      updater.outputs_in_stable_memory,
      updater.outputs_https_outcall,
    );

    {
      for (outpoint, mut utxo_entry) in utxo_cache {
        if Index::is_special_outpoint(outpoint) {
          if let Some(old_entry) = mem_get_outpoint_to_utxo_entry(outpoint.store()) {
            utxo_entry = UtxoEntryBuf::merged(&old_entry, &utxo_entry, index);
          }
        }

        mem_insert_outpoint_to_utxo_entry(outpoint.store(), utxo_entry.as_ref().clone());

        let utxo_entry = utxo_entry.parse(index);
        if index.index_addresses {
          let script_pubkey = utxo_entry.script_pubkey();
          mem_insert_script_pubkey_to_outpoints(script_pubkey.to_vec(), outpoint);
        }

        if index.index_inscriptions {
          for (sequence_number, offset) in utxo_entry.parse_inscriptions() {
            let satpoint = SatPoint { outpoint, offset };
            mem_insert_sequence_number_to_satpoint(sequence_number, satpoint.store());
          }
        }
      }
    }

    mem_increment_statistic(Statistic::OutputsTraversed, updater.outputs_traversed);
    mem_increment_statistic(Statistic::SatRanges, updater.sat_ranges_since_flush);
    mem_increment_statistic(Statistic::Commits, 1);
    Ok(())
  }

  async fn get_raw_transaction_info_forever(txid: Txid) -> Result<GetRawTransactionResult> {
    let mut retry_count = 1;
    loop {
      match crate::rpc::get_raw_transaction_info(&txid, None).await {
        Ok(info) => return Ok(info),
        Err(e) => {
          log!(ERROR, "failed to get_raw_transaction_info: {:?} error: {:?}", txid, e);
        }
      }
      log!(INFO, "retrying +{} get_raw_transaction_info: {:?}", retry_count, txid,);
      retry_count = retry_count + 1;
    }
  }