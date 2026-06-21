use crate::http::{RequestDiagnostics, retry_after, should_retry_status};
use reqwest::{
    StatusCode,
    header::{HeaderMap, HeaderValue, RETRY_AFTER},
};
use std::time::Duration;

#[test]
fn retries_only_transient_statuses() {
    assert!(should_retry_status(StatusCode::TOO_MANY_REQUESTS));
    assert!(should_retry_status(StatusCode::SERVICE_UNAVAILABLE));
    assert!(should_retry_status(StatusCode::GATEWAY_TIMEOUT));
    assert!(!should_retry_status(StatusCode::BAD_REQUEST));
    assert!(!should_retry_status(StatusCode::UNAUTHORIZED));
    assert!(!should_retry_status(
        StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE
    ));
}

#[test]
fn parses_retry_after_seconds() {
    let mut headers = HeaderMap::new();
    headers.insert(RETRY_AFTER, HeaderValue::from_static("2"));

    assert_eq!(retry_after(&headers), Some(Duration::from_secs(2)));
}

#[test]
fn request_diagnostics_are_size_only() {
    let diagnostics = RequestDiagnostics::new(100, 20, 30, 40, 50);

    assert_eq!(
        diagnostics.to_string(),
        "payload_bytes=100, system_bytes=20, index_bytes=30, bodies_bytes=40, auth_header_bytes=50"
    );
}
