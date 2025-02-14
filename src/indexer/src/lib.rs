mod memory;
mod rpc;
mod config;
mod bitcoin_api;

use anyhow::Error;

type Result<T = (), E = Error> = std::result::Result<T, E>;

#[ic_cdk::query]
fn greet(name: String) -> String {
    format!("Hello, {}!", name)
}
