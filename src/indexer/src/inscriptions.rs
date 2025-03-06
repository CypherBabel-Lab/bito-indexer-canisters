use std::{fmt::{self, Display, Formatter}, str::FromStr};

use anyhow::{Error, anyhow};
use ordinals::{Charm, Sat, SatPoint, SpacedRune};
use serde::{Deserialize, Serialize};
use crate::re;

pub use self::{inscription::Inscription, inscription_id::InscriptionId};
pub(crate) mod envelope;
mod inscription;
pub(crate) mod inscription_id;
pub(crate) mod media;
mod tag;
pub(crate) mod teleburn;


#[derive(Copy, Clone, Debug)]
pub(crate) enum InscriptionQuery {
  Id(InscriptionId),
  Number(i32),
  Sat(Sat),
}

impl FromStr for InscriptionQuery {
  type Err = Error;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    if re::INSCRIPTION_ID.is_match(s) {
      Ok(Self::Id(s.parse()?))
    } else if re::INSCRIPTION_NUMBER.is_match(s) {
      Ok(Self::Number(s.parse()?))
    } else if re::SAT_NAME.is_match(s) {
      Ok(Self::Sat(s.parse()?))
    } else {
      Err(anyhow!("bad inscription query {s}"))
    }
  }
}

impl Display for InscriptionQuery {
  fn fmt(&self, f: &mut Formatter) -> fmt::Result {
    match self {
      Self::Id(id) => write!(f, "{id}"),
      Self::Number(number) => write!(f, "{number}"),
      Self::Sat(sat) => write!(f, "on sat {}", sat.name()),
    }
  }
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct InscriptionResp {
  pub address: Option<String>,
  pub charms: Vec<Charm>,
  pub child_count: u64,
  pub children: Vec<InscriptionId>,
  pub content_length: Option<usize>,
  pub content_type: Option<String>,
  pub effective_content_type: Option<String>,
  pub fee: u64,
  pub height: u32,
  pub id: InscriptionId,
  pub next: Option<InscriptionId>,
  pub number: i32,
  pub parents: Vec<InscriptionId>,
  pub previous: Option<InscriptionId>,
  pub rune: Option<SpacedRune>,
  pub sat: Option<ordinals::Sat>,
  pub satpoint: SatPoint,
  pub timestamp: i64,
  pub value: Option<u64>,
  pub metaprotocol: Option<String>,
}
