use candid::{CandidType, Deserialize, Principal};
use ic_cdk::api::management_canister::bitcoin::BitcoinNetwork;
use ic_stable_structures::storable::{Bound, Storable};
use serde::Serialize;
use std::borrow::Cow;

#[derive(CandidType, Deserialize, Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Config {
  pub network: BitcoinNetwork,
  pub bitcoin_rpc_url: String,
  pub subscribers: Vec<Principal>,
  pub index_addresses: Option<bool>,
  pub index_sats: Option<bool>,
  pub index_runes: Option<bool>,
  pub index_inscriptions: Option<bool>,
  pub index_transactions: Option<bool>,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      network: BitcoinNetwork::Regtest,
      bitcoin_rpc_url: "".to_string(),
      subscribers: vec![],
      index_addresses: Some(false),
      index_sats: Some(false),
      index_runes: Some(true), 
      index_inscriptions: Some(true),
      index_transactions: Some(false),
    }
  }
}

impl Config {
  pub fn get_subnet_nodes(&self) -> u64 {
    match self.network {
      BitcoinNetwork::Regtest => 13,
      BitcoinNetwork::Testnet => 13,
      BitcoinNetwork::Mainnet => 34,
    }
  }
}

impl Storable for Config {
  fn to_bytes(&self) -> Cow<[u8]> {
    let bytes = bincode::serialize(self).unwrap();
    Cow::Owned(bytes)
  }

  fn from_bytes(bytes: std::borrow::Cow<[u8]>) -> Self {
    bincode::deserialize(bytes.as_ref()).unwrap()
  }

  const BOUND: Bound = Bound::Unbounded;
}

#[derive(CandidType, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct UpgradeArgs {
  pub bitcoin_rpc_url: Option<String>,
  pub subscribers: Option<Vec<Principal>>,
}

#[derive(CandidType, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum InitIndexerArgs {
  Init(Config),
  Upgrade(Option<UpgradeArgs>),
}
