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