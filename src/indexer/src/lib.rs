mod api;
mod memory;
mod rpc;
mod config;
mod chain;
mod bitcoin_api;
mod index;
mod inscriptions;
mod macros;
mod notifier;
mod error;
mod test;
mod re;
mod runes;

use anyhow::Error;
use bitcoin::{hashes::Hash, OutPoint};
use chrono::{DateTime, TimeZone, Utc};

type Result<T = (), E = Error> = std::result::Result<T, E>;

fn default<T: Default>() -> T {
    Default::default()
}

pub fn unbound_outpoint() -> OutPoint {
    OutPoint {
      txid: Hash::all_zeros(),
      vout: 0,
    }
}

fn timestamp(seconds: u64) -> DateTime<Utc> {
    Utc
      .timestamp_opt(seconds.try_into().unwrap_or(i64::MAX), 0)
      .unwrap()
  }