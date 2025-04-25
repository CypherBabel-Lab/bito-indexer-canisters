use candid::{CandidType, Deserialize};
use serde::Serialize;

#[derive(Debug, CandidType, Deserialize)]
pub struct InscriptionEntry {
    pub charms: u16,
    pub fee: u64,
    pub height: u32,
    pub id: String,
    pub inscription_number: i32,
    pub parents: Vec<u32>,
    pub sat: u64,
    pub sequence_number: u32,
    pub timestamp: u32,
}

#[derive(Clone, Debug,CandidType, Serialize, Deserialize)]
pub enum InscriptionQuery {
  Id(String),
  Number(i32),
  Sat(String),
}


#[derive(Debug, CandidType, Deserialize, Clone)]
pub struct Inscription {
  pub address: Option<String>,
  pub charms: Vec<String>,
  pub child_count: u64,
  pub children: Vec<String>,
  pub content_length: Option<usize>,
  pub content_type: Option<String>,
  pub effective_content_type: Option<String>,
  pub fee: u64,
  pub height: u32,
  pub id: String,
  pub next: Option<String>,
  pub number: i32,
  pub parents: Vec<String>,
  pub previous: Option<String>,
  pub rune: Option<String>,
  pub sat: Option<u64>,
  pub satpoint: String,
  pub timestamp: i64,
  pub value: Option<u64>,
  pub metaprotocol: Option<String>,
}


#[derive(Debug, CandidType, Deserialize)]
pub struct RuneBalance {
  pub confirmations: u32,
  pub rune_id: String,
  pub amount: u128,
  pub divisibility: u8,
  pub symbol: Option<String>,
}

#[derive(Debug, CandidType, Deserialize)]
pub struct GetEtchingResult {
  pub confirmations: u32,
  pub rune_id: String,
}

#[derive(Debug, CandidType, Deserialize)]
pub struct Terms {
  pub amount: Option<u128>,
  pub cap: Option<u128>,
  pub height: (Option<u64>, Option<u64>),
  pub offset: (Option<u64>, Option<u64>),
}

#[derive(Debug, CandidType, Deserialize)]
pub struct RuneEntry {
  pub confirmations: u32,
  pub rune_id: String,
  pub block: u64,
  pub burned: u128,
  pub divisibility: u8,
  pub etching: String,
  pub mints: u128,
  pub number: u64,
  pub premine: u128,
  pub spaced_rune: String,
  pub symbol: Option<String>,
  pub terms: Option<Terms>,
  pub timestamp: u64,
  pub turbo: bool,
}

#[derive(Debug, CandidType, Deserialize)]
pub enum Error {
  MaxOutpointsExceeded,
}