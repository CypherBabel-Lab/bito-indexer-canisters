use std::str::FromStr;

use candid::{candid_method, Principal};
use ic_canister_log::log;
use ic_canisters_http_types::{HttpRequest, HttpResponse};
use ic_cdk::{api::management_canister::http_request::{HttpResponse as HttpResponse2 , TransformArgs}, init, post_upgrade, query, update};
use logs::INFO;
use indexer_interface::{InscriptionEntry, InscriptionQuery as InscriptionQueryApi, Inscription as InscriptionApi};

use crate::{
    config::InitIndexerArgs, 
    index::{self, cancel_shutdown, inscription_info, mem_get_config, mem_get_inscription_id_to_sequence_number, mem_get_sequence_number_to_inscription_entry, mem_latest_block, mem_set_config, shut_down, updater::update_index, Index}, 
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
#[candid_method(query)]
pub fn get_latest_block() -> (u32, String) {
  let (height, hash) = mem_latest_block().expect("No block found");
  (height, hash.to_string())
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