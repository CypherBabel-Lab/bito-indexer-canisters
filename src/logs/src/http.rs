use ic_canisters_http_types::{HttpRequest, HttpResponse, HttpResponseBuilder};
use super::{Log, Priority, Sort};
use std::str::FromStr;

const MAX_BODY_SIZE: usize = 3_000_000;

pub fn to_http_response(req: &HttpRequest) -> HttpResponse {
    let max_skip_timestamp = match req.raw_query_param("time") {
        Some(arg) => match u64::from_str(arg) {
            Ok(value) => value,
            Err(_) => {
                return HttpResponseBuilder::bad_request()
                    .with_body_and_content_length("failed to parse the 'time' parameter")
                    .build();
            }
        },
        None => 0,
    };

    let mut log: Log = Default::default();

    match req.raw_query_param("priority") {
        Some(priority_str) => match Priority::from_str(priority_str) {
            Ok(priority) => match priority {
                Priority::Error => log.push_logs(Priority::Error),
                Priority::Info => log.push_logs(Priority::Info),
                Priority::TraceHttp => log.push_logs(Priority::TraceHttp),
                Priority::Debug => log.push_logs(Priority::Debug),
            },
            Err(_) => log.push_all(),
        },
        None => log.push_all(),
    }

    log.entries
        .retain(|entry| entry.timestamp >= max_skip_timestamp);

    log.sort_logs(ordering_from_query_params(
        req.raw_query_param("sort"),
        max_skip_timestamp,
    ));
    HttpResponseBuilder::ok()
        .header("Content-Type", "application/json; charset=utf-8")
        .with_body_and_content_length(log.serialize_logs(MAX_BODY_SIZE))
        .build()
}

fn ordering_from_query_params(sort: Option<&str>, max_skip_timestamp: u64) -> Sort {
    match sort {
        Some(ord_str) => match Sort::from_str(ord_str) {
            Ok(order) => order,
            Err(_) => {
                if max_skip_timestamp == 0 {
                    Sort::Ascending
                } else {
                    Sort::Descending
                }
            }
        },
        None => {
            if max_skip_timestamp == 0 {
                Sort::Ascending
            } else {
                Sort::Descending
            }
        }
    }
}
