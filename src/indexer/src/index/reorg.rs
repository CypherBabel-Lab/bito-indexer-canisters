use std::fmt::{self, Display, Formatter};

use bitcoin::BlockHash;
use ic_canister_log::log;
use ic_cdk::api::management_canister::bitcoin::BitcoinNetwork;
use logs::INFO;

use super::{mem_block_hash, updater::BlockData, Index};

use crate::{bitcoin_api::get_block_hash, index::{entry::Entry, mem_prune_block_header, mem_prune_change_record_rune, mem_prune_statistic_reserved_runes, mem_prune_statistic_runes}};

#[derive(Debug, PartialEq)]
pub(crate) enum Error {
  Recoverable { height: u32, depth: u32 },
  Unrecoverable,
}

impl Display for Error {
  fn fmt(&self, f: &mut Formatter) -> fmt::Result {
    match self {
      Self::Recoverable { height, depth } => {
        write!(f, "{depth} block deep reorg detected at height {height}")
      }
      Self::Unrecoverable => write!(f, "unrecoverable reorg detected"),
    }
  }
}

impl std::error::Error for Error {}

const MAX_RECOVERABLE_REORG_DEPTH: u32 = 6;

pub(crate) struct Reorg {}

impl Reorg {
  pub(crate) async fn detect_reorg(network: BitcoinNetwork, index_prev_blockhash: Option<BlockHash>, block: &BlockData, height: u32,) -> Result<(), Error> {
    let bitcoind_prev_blockhash = block.header.prev_blockhash;
    match index_prev_blockhash {
      Some(index_prev_blockhash) if index_prev_blockhash == bitcoind_prev_blockhash => Ok(()),
      Some(index_prev_blockhash) if index_prev_blockhash != bitcoind_prev_blockhash => {
        for depth in 1..MAX_RECOVERABLE_REORG_DEPTH {
          let index_block_hash = mem_block_hash(height.checked_sub(depth).expect("height overflow"))
            .ok_or(Error::Unrecoverable)?;
          let bitcoin_height = height.checked_sub(depth).expect("height overflow");
          let block_hash = get_block_hash(network, bitcoin_height)
            .await
            .map_err(|_| Error::Unrecoverable)?;

          let bitcoind_block_hash =  block_hash.ok_or(Error::Unrecoverable)?;

          if index_block_hash == bitcoind_block_hash {
            return Err(Error::Recoverable { height, depth });
          }
        }

        Err(Error::Unrecoverable)
      }
      _ => Ok(()),
    }
  }

  pub fn handle_reorg(height: u32, depth: u32, index: &Index) {
    log!(
      INFO,
      "rolling back state after reorg of depth {depth} at height {height}"
    );

    for h in (height - depth + 1..height).rev() {
      log!(INFO, "rolling back change record at height {h}");
      if index.index_runes && h >= index.first_rune_height() {
        if let Some(change_record) = crate::index::mem_get_change_record_rune(h) {
          change_record
            .removed_outpoints
            .iter()
            .for_each(|(outpoint, rune_balances, height)| {
              crate::index::mem_insert_outpoint_to_rune_balances(
                outpoint.store(),
                rune_balances.clone(),
              );
              crate::index::mem_insert_outpoint_to_height(outpoint.store(), *height);
            });
          change_record.added_outpoints.iter().for_each(|outpoint| {
            crate::index::mem_remove_outpoint_to_rune_balances(outpoint.store());
            crate::index::mem_remove_outpoint_to_height(outpoint.store());
          });
          change_record.burned.iter().for_each(|(rune_id, amount)| {
            let mut entry = crate::index::mem_get_rune_id_to_rune_entry(rune_id.store()).unwrap();
            entry.burned = *amount;
            crate::index::mem_insert_rune_id_to_rune_entry(rune_id.store(), entry);
            log!(
              INFO,
              "resetting burned for rune_id: {} to {}",
              rune_id,
              amount
            );
          });
          change_record.mints.iter().for_each(|(rune_id, amount)| {
            let mut entry = crate::index::mem_get_rune_id_to_rune_entry(rune_id.store()).unwrap();
            entry.mints = *amount;
            crate::index::mem_insert_rune_id_to_rune_entry(rune_id.store(), entry);
            log!(
              INFO,
              "resetting mints for rune_id: {} to {}",
              rune_id,
              amount
            );
          });
          change_record
            .added_runes
            .iter()
            .for_each(|(rune, rune_id, txid)| {
              crate::index::mem_remove_rune_to_rune_id(rune.store());
              crate::index::mem_remove_rune_id_to_rune_entry(rune_id.store());
              crate::index::mem_remove_transaction_id_to_rune(txid.store());
              log!(INFO, "removing rune_id: {}", rune_id);
            });
        }
        crate::index::mem_remove_change_record_rune(h);
        crate::index::mem_remove_statistic_runes(h);
        crate::index::mem_remove_statistic_reserved_runes(h);
        crate::index::mem_remove_block_header(h);
      }
      if index.index_inscriptions || index.index_addresses || index.index_sats {
        // TODO: implement
      }
    }

    log!(
      INFO,
      "successfully rolled back state to height {}",
      height - depth,
    );
  }

  pub(crate) fn prune_change_record(height: u32) {
    if height >= MAX_RECOVERABLE_REORG_DEPTH {
      let h = height - MAX_RECOVERABLE_REORG_DEPTH;
      log!(INFO, "clearing change record at height {h}");
      mem_prune_change_record_rune(h);
      mem_prune_statistic_runes(h);
      mem_prune_statistic_reserved_runes(h);
      mem_prune_block_header(h);
    }
  }
}