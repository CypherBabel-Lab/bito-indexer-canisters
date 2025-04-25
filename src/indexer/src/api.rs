use std::str::FromStr;

use bitcoin::{OutPoint, Txid};
use candid::{candid_method, Principal};
use ic_canister_log::log;
use ic_canisters_http_types::{HttpRequest, HttpResponse};
use ic_cdk::{api::management_canister::http_request::{HttpResponse as HttpResponse2 , TransformArgs}, init, post_upgrade, query, update};
use logs::{ERROR, INFO};
use indexer_interface::{Error, GetEtchingResult, Inscription as InscriptionApi, InscriptionEntry, InscriptionQuery as InscriptionQueryApi, RuneBalance, RuneEntry, Terms};

use crate::{
    config::InitIndexerArgs, 
    index::{self, cancel_shutdown, entry::Entry, inscription_info, mem_get_config, mem_get_etching, mem_get_inscription_id_to_sequence_number, mem_get_outpoint_to_height, mem_get_outpoint_to_rune_balances, mem_get_rune_id_to_rune_entry, mem_get_rune_to_rune_id, mem_get_sequence_number_to_inscription_entry, mem_latest_block, mem_latest_block_height, mem_set_config, shut_down, updater::update_index, Index}, 
    inscriptions::{InscriptionId, InscriptionQuery}, rpc::should_keep
};

#[init]
#[candid_method(init)]
fn init(init_indexer_args: InitIndexerArgs) {
  match init_indexer_args {
    InitIndexerArgs::Init(mut config) => {
      if config.index_inscriptions.is_none() {
        config.index_inscriptions = Some(true);
      }
    if config.index_runes.is_none() {
        config.index_runes = Some(true);
    }
      mem_set_config(config).unwrap();
    }
    InitIndexerArgs::Upgrade(_) => ic_cdk::trap(
      "Cannot initialize the canister with an Upgrade argument. Please provide an Init argument.",
    ),
  }
}

#[post_upgrade]
fn post_upgrade(runes_indexer_args: Option<InitIndexerArgs>) {
  match runes_indexer_args {
    Some(InitIndexerArgs::Upgrade(Some(upgrade_args))) => {
      let mut config = mem_get_config();
      if let Some(bitcoin_rpc_url) = upgrade_args.bitcoin_rpc_url {
        config.bitcoin_rpc_url = bitcoin_rpc_url;
      }
      if let Some(subscribers) = upgrade_args.subscribers {
        config.subscribers = subscribers;
        log!(INFO, "subscribers updated: {:?}", config.subscribers);
      }
      mem_set_config(config).unwrap();
    }
    None | Some(InitIndexerArgs::Upgrade(None)) => {}
    _ => ic_cdk::trap(
      "Cannot upgrade the canister with an Init argument. Please provide an Upgrade argument.",
    ),
  }
}

#[query]
pub fn get_latest_block() -> (u32, String) {
  let (height, hash) = mem_latest_block().expect("No block found");
  (height, hash.to_string())
}

#[query]
pub fn get_etching(txid: String) -> Option<GetEtchingResult> {
  let txid = Txid::from_str(&txid).ok()?;
  let cur_height = mem_latest_block_height().expect("No block height found");

  mem_get_etching(txid).map(|(id, entry)| GetEtchingResult {
    confirmations: cur_height - entry.block as u32 + 1,
    rune_id: id.to_string(),
  })
}

#[query]
pub fn get_rune(str_spaced_rune: String) -> Option<RuneEntry> {
  let spaced_rune = ordinals::SpacedRune::from_str(&str_spaced_rune).ok()?;
  let rune_id_value = mem_get_rune_to_rune_id(spaced_rune.rune.0)?;
  let rune_entry = mem_get_rune_id_to_rune_entry(rune_id_value)?;
  let cur_height = mem_latest_block_height().expect("No block height found");
  Some(RuneEntry {
    confirmations: cur_height - rune_entry.block as u32 + 1,
    rune_id: ordinals::RuneId::load(rune_id_value).to_string(),
    block: rune_entry.block,
    burned: rune_entry.burned,
    divisibility: rune_entry.divisibility,
    etching: rune_entry.etching.to_string(),
    mints: rune_entry.mints,
    number: rune_entry.number,
    premine: rune_entry.premine,
    spaced_rune: rune_entry.spaced_rune.to_string(),
    symbol: rune_entry.symbol.map(|c| c.to_string()),
    terms: rune_entry.terms.map(|t| Terms {
      amount: t.amount,
      cap: t.cap,
      height: t.height,
      offset: t.offset,
    }),
    timestamp: rune_entry.timestamp,
    turbo: rune_entry.turbo,
  })
}

#[query]
pub fn get_rune_by_id(str_rune_id: String) -> Option<RuneEntry> {
  let rune_id = ordinals::RuneId::from_str(&str_rune_id).ok()?;
  let rune_entry = mem_get_rune_id_to_rune_entry(rune_id.store())?;
  let cur_height = mem_latest_block_height().expect("No block height found");
  Some(RuneEntry {
    confirmations: cur_height - rune_entry.block as u32 + 1,
    rune_id: str_rune_id,
    block: rune_entry.block,
    burned: rune_entry.burned,
    divisibility: rune_entry.divisibility,
    etching: rune_entry.etching.to_string(),
    mints: rune_entry.mints,
    number: rune_entry.number,
    premine: rune_entry.premine,
    spaced_rune: rune_entry.spaced_rune.to_string(),
    symbol: rune_entry.symbol.map(|c| c.to_string()),
    terms: rune_entry.terms.map(|t| Terms {
      amount: t.amount,
      cap: t.cap,
      height: t.height,
      offset: t.offset,
    }),
    timestamp: rune_entry.timestamp,
    turbo: rune_entry.turbo,
  })
}

#[query]
pub fn get_rune_balances_for_outputs(
  outpoints: Vec<String>,
) -> Result<Vec<Option<Vec<RuneBalance>>>, Error> {
  if outpoints.len() > 64 {
    return Err(Error::MaxOutpointsExceeded);
  }

  let cur_height = mem_latest_block_height().expect("No block height found");
  let mut piles = Vec::new();

  for str_outpoint in outpoints {
    let outpoint = match OutPoint::from_str(&str_outpoint) {
      Ok(o) => o,
      Err(e) => {
        log!(ERROR, "Failed to parse outpoint {}: {}", str_outpoint, e);
        piles.push(None);
        continue;
      }
    };
    let k = OutPoint::store(outpoint);
    if let Some(rune_balances) = mem_get_outpoint_to_rune_balances(k) {
      if let Some(height) = mem_get_outpoint_to_height(k) {
        let confirmations = cur_height - height + 1;

        let mut outpoint_balances = Vec::new();
        for rune_balance in rune_balances.balances.iter() {
          let rune_entry =
            mem_get_rune_id_to_rune_entry(rune_balance.rune_id.store());
          if let Some(rune_entry) = rune_entry {
            outpoint_balances.push(RuneBalance {
              confirmations,
              rune_id: rune_balance.rune_id.to_string(),
              amount: rune_balance.balance,
              divisibility: rune_entry.divisibility,
              symbol: rune_entry.symbol.map(|c| c.to_string()),
            });
          } else {
            log!(
              ERROR,
              "Rune not found for rune_id {}",
              rune_balance.rune_id.to_string()
            );
          }
        }
        piles.push(Some(outpoint_balances));
      } else {
        log!(ERROR, "Height not found for outpoint {}", str_outpoint);
        piles.push(None);
      }
    } else {
      log!(
        ERROR,
        "Rune balances not found for outpoint {}",
        str_outpoint
      );
      piles.push(None);
    }
  }

  Ok(piles)
}

#[update]
pub async fn get_inscription_info(arg: InscriptionQueryApi, child: Option<usize>) -> Result<Option<InscriptionApi>, String> {
  let query = match arg {
    InscriptionQueryApi::Id(id) => {
      let inscription_id = match InscriptionId::from_str(&id) {
        Ok(id) => id,
        Err(e) => return Err(e.to_string()),
      };
      InscriptionQuery::Id(inscription_id)
    }
    InscriptionQueryApi::Number(seq) => {
      InscriptionQuery::Number(seq)
    }
    InscriptionQueryApi::Sat(sat) => {
      InscriptionQuery::Sat(sat.parse().map_err(|e: ordinals::sat::Error| e.to_string())?)
    }
  };
  let index = Index::from_config(&mem_get_config());
  if let InscriptionQuery::Sat(_) = query {
    if !index.has_sat_index() {
      return Err("Sat index is disabled".to_string());
    }
  }
  let info = inscription_info(&index, query, child).await.map_err(|e| e.to_string())?;
  match info {
    Some((info,_,_)) => Ok(Some(InscriptionApi {
      address: info.address,
      charms: info.charms.iter().map(|c| c.to_string()).collect(),
      child_count: info.child_count,
      children: info.children.iter().map(|id| id.to_string()).collect(),
      content_length: info.content_length,
      content_type: info.content_type,
      effective_content_type: info.effective_content_type,
      fee: info.fee,
      height: info.height,
      id: info.id.to_string(),
      next: info.next.map(|id| id.to_string()),
      number: info.number,
      parents: info.parents.iter().map(|id| id.to_string()).collect(),
      previous: info.previous.map(|id| id.to_string()),
      rune: info.rune.map(|rune| rune.to_string()),
      sat: info.sat.map(|sat| sat.n()),
      satpoint: info.satpoint.to_string(),
      timestamp: info.timestamp,
      value: info.value,
      metaprotocol: info.metaprotocol,
    })),
    None => Ok(None),    
  }
}

#[query]
pub fn get_inscription_entry(inscription_id_str: String) -> Result<Option<InscriptionEntry>, String> {
  let inscription_id = match InscriptionId::from_str(&inscription_id_str) {
    Ok(id) => id,
    Err(e) => return Err(e.to_string()),
  };
  let Some(sequence_number) = mem_get_inscription_id_to_sequence_number(&inscription_id) else {
    return Ok(None);
  };
  let entry = mem_get_sequence_number_to_inscription_entry(sequence_number);
  match entry {
    Some(entry) => Ok(Some(InscriptionEntry {
        charms: entry.charms,
        fee: entry.fee,
        height: entry.height,
        id: inscription_id_str,
        inscription_number: entry.inscription_number,
        parents: entry.parents,
        sat: match entry.sat {
            Some(sat) => sat.n(),
            None => 0,
            
        },
        sequence_number,
        timestamp: entry.timestamp,
    })),
    None => Ok(None),    
  }
}

#[query]
pub fn get_inscriptions_in_block(block_height: u32) -> Result<Vec<String>, String> {
  let r = index::get_inscriptions_in_block(block_height);
  match r {
    Ok(inscriptions) => Ok(inscriptions.iter().map(|id| id.to_string()).collect()),
    Err(e) => Err(e.to_string()),
  }
}


#[update(hidden = true)]
pub fn start() -> Result<(), String> {
  let caller = ic_cdk::api::caller();
  if !ic_cdk::api::is_controller(&caller) {
    return Err("Not authorized".to_string());
  }

  cancel_shutdown();
  let config = mem_get_config();
  let indexer = Index::from_config(&config);
  let _ = update_index(config.network, indexer, config.subscribers);
  Ok(())
}

#[query(hidden = true)]
pub fn rpc_transform(args: TransformArgs) -> HttpResponse2 {
  let headers = args
    .response
    .headers
    .into_iter()
    .filter(|h| should_keep(h.name.as_str()))
    .collect::<Vec<_>>();
  HttpResponse2 {
    status: args.response.status.clone(),
    body: args.response.body.clone(),
    headers,
  }
}

#[update(hidden = true)]
pub fn stop() -> Result<(), String> {
  let caller = ic_cdk::api::caller();
  if !ic_cdk::api::is_controller(&caller) {
    return Err("Not authorized".to_string());
  }

  shut_down();
  log!(INFO, "Waiting for index thread to finish...");

  Ok(())
}

#[update(hidden = true)]
pub fn set_bitcoin_rpc_url(url: String) -> Result<(), String> {
  let caller = ic_cdk::api::caller();
  if !ic_cdk::api::is_controller(&caller) {
    return Err("Not authorized".to_string());
  }
  let mut config = mem_get_config();
  config.bitcoin_rpc_url = url;
  mem_set_config(config).unwrap();

  Ok(())
}

#[query(hidden = true)]
pub fn get_subscribers() -> Vec<Principal> {
  mem_get_config().subscribers
}

#[ic_cdk::query(hidden = true)]
fn http_request(req: HttpRequest) -> HttpResponse {
    use logs::http;
    http::to_http_response(&req)
}

ic_cdk::export_candid!();