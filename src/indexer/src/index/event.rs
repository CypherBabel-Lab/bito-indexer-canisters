use std::borrow::Cow;

use bitcoin::{OutPoint, Txid};
use ic_stable_structures::{storable::Bound, Storable};
use ordinals::{RuneId, SatPoint};
use serde::{Deserialize, Serialize};

use crate::inscriptions::InscriptionId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Event {
  InscriptionCreated {
    block_height: u32,
    charms: u16,
    inscription_id: InscriptionId,
    location: Option<SatPoint>,
    parent_inscription_ids: Vec<InscriptionId>,
    sequence_number: u32,
  },
  InscriptionTransferred {
    block_height: u32,
    inscription_id: InscriptionId,
    new_location: SatPoint,
    old_location: SatPoint,
    sequence_number: u32,
  },
  RuneBurned {
    amount: u128,
    block_height: u32,
    rune_id: RuneId,
    txid: Txid,
  },
  RuneEtched {
    block_height: u32,
    rune_id: RuneId,
    txid: Txid,
  },
  RuneMinted {
    amount: u128,
    block_height: u32,
    rune_id: RuneId,
    txid: Txid,
  },
  RuneTransferred {
    amount: u128,
    block_height: u32,
    outpoint: OutPoint,
    rune_id: RuneId,
    txid: Txid,
  },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Events(pub Vec<Event>);

impl Events {
  pub fn new() -> Self {
    Self(Vec::new())
  }

  pub fn push(&mut self, event: Event) {
    self.0.push(event)
  }

  pub fn extend(&mut self, events: Events) {
    self.0.extend(events.0)
  }

  pub fn iter(&self) -> impl Iterator<Item = &Event> {
    self.0.iter()
  }
}

impl Storable for Events {
  fn to_bytes(&self) -> Cow<[u8]> {
    let vec = bincode::serialize(self).unwrap();
    Cow::Owned(vec)
  }

  fn from_bytes(bytes: Cow<[u8]>) -> Self {
    bincode::deserialize(&bytes).unwrap()
  }

  const BOUND: Bound = Bound::Unbounded;
}