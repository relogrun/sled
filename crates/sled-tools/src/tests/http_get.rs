use crate::http_get::validate_url;

#[test]
fn validates_public_http_urls() {
    assert!(validate_url("https://example.com").is_ok());
    assert!(validate_url("http://93.184.216.34").is_ok());
}

#[test]
fn rejects_unsupported_schemes() {
    assert!(validate_url("file:///etc/passwd").is_err());
    assert!(validate_url("ftp://example.com/file").is_err());
}

#[test]
fn rejects_local_hosts_and_private_ips() {
    assert!(validate_url("http://localhost:8080").is_err());
    assert!(validate_url("http://api.localhost").is_err());
    assert!(validate_url("http://127.0.0.1").is_err());
    assert!(validate_url("http://10.0.0.1").is_err());
    assert!(validate_url("http://192.168.1.1").is_err());
    assert!(validate_url("http://[::1]").is_err());
    assert!(validate_url("http://[fd00::1]").is_err());
}
