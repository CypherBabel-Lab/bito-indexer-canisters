mod cache;
mod cli;
mod proxy;

use base64;
use base64::Engine;
use cache::LruCache;
use clap::Parser;
use http_body_util::BodyExt;
use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{
  header::{HeaderValue, RANGE},
  Request, Response, StatusCode,
};
use hyper_util::rt::TokioIo;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;

fn try_match_range_header(req: &Request<Incoming>) -> Option<(usize, usize)> {
  if let Some(range_control) = req.headers().get(RANGE).map(|v| v.to_str().ok()).flatten() {
    let range = range_control
      .trim_start_matches("bytes=")
      .split('-')
      .collect::<Vec<&str>>();
    let start = range[0].parse::<usize>().ok()?;
    let end = range[1].parse::<usize>().ok()?;
    (end >= start).then(|| (start, end))
  } else {
    None
  }
}

fn try_match_cache_header(req: &Request<Incoming>) -> Option<String> {
  req
    .headers()
    .get("Idempotency-Key")
    .map(|v| v.to_str().ok())
    .flatten()
    .map(|key| key.to_string())
}

async fn forward(
  target: impl AsRef<str>,
  user: Option<String>,
  mut req: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, Infallible> {
  let if_range = try_match_range_header(&req);
  if let Some(user) = user {
    let base64_credentials = base64::engine::general_purpose::STANDARD.encode(user);
    req.headers_mut().insert(
      "Authorization",
      HeaderValue::from_str(&format!("Basic {}", base64_credentials)).unwrap(),
    );
  }
  match proxy::call(target.as_ref(), req).await {
    Ok(response) => match if_range {
      Some((start, end)) => {
        let body = response
          .collect()
          .await?
          .to_bytes()
          .iter()
          .copied()
          .collect::<Vec<u8>>();
        if body.len() <= end - start + 1 {
          Ok(
            Response::builder()
              .status(StatusCode::OK)
              .body(Full::from(body))
              .unwrap(),
          )
        } else {
          let partial = if end >= body.len() {
            body[start..].to_vec()
          } else {
            body[start..=end].to_vec()
          };
          Ok(
            Response::builder()
              .status(StatusCode::PARTIAL_CONTENT)
              .header(
                "Content-Range",
                format!("bytes {}-{}/{}", start, end, body.len()),
              )
              .header("Content-Length", partial.len().to_string())
              .body(Full::from(Bytes::from(partial)))
              .unwrap(),
          )
        }
      }
      None => Ok(response),
    },
    Err(error) => {
      println!("{:?}", error);
      Ok(
        Response::builder()
          .status(StatusCode::INTERNAL_SERVER_ERROR)
          .body(Full::from(Bytes::from("Internal Server Error")))
          .unwrap(),
      )
    }
  }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
  let args = cli::Cli::parse();
  let addr = SocketAddr::from((args.run.addr, args.run.port));
  let target = args.run.forward;
  let user = args.run.user;
  let listener = TcpListener::bind(addr).await?;
  let cache = Arc::new(cache::MemoryCache::<Response<Full<Bytes>>>::new(1000));
  loop {
    let cache = cache.clone();
    let target = target.clone();
    let user = user.clone();
    let (stream, _) = listener.accept().await?;
    let io = TokioIo::new(stream);
    tokio::task::spawn(async move {
      let f = |req| async {
        let key = try_match_cache_header(&req);
        if let Some(ref key) = key {
          match cache.get(key).await {
            Some(response) => {
              println!("Cache hit {}", key);
              return Ok(response);
            }
            None => {}
          }
        }
        let rsp = forward(&target, user.clone(), req).await;
        let Ok(ref response) = rsp;
          // if let Some(key) = key {
          //   println!("Cache created {}", key);
          //   cache.put(key, response.clone()).await;
          // }
          if response.status().is_success() {
            if let Some(key) = key {
              println!("Cache created {}", key);
              cache.put(key, response.clone()).await;
            }
          }
        rsp
      };
      if let Err(err) = http1::Builder::new()
        .serve_connection(io, service_fn(f))
        .await
      {
        eprintln!("Error serving connection: {:?}", err);
      }
    });
  }
}
