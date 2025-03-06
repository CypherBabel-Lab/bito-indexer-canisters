use std::fmt::{self, Display, Formatter};

use bitcoin::BlockHash;
use ic_cdk::api::management_canister::bitcoin::BitcoinNetwork;

use super::{mem_block_hash, updater::BlockData};

use crate::bitcoin_api::get_block_hash;

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

  pub(crate) fn handle_reorg(_network: BitcoinNetwork, _height: u32, _depth: u32) {
  }
}