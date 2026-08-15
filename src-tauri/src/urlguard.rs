//! URL validation for every outgoing request Advoco makes.
//!
//! Rules (SSRF guard):
//! - Only `http`/`https` schemes are ever allowed.
//! - User-supplied endpoints (LLM provider base URLs) must additionally point
//!   at a public host: loopback, private, shared, reserved, and
//!   link-local addresses are rejected — UNLESS the user has explicitly
//!   flipped the "local model endpoint" toggle for a local inference server
//!   (Ollama, llama.cpp, LM Studio).
//! - The Proto-Familiar target is NOT user-supplied input: it is the fixed
//!   constant [`PF_BASE_URL`] (the product's intrinsic function), so it is
//!   validated only for scheme and shape, never against the private-address
//!   rule.

use std::net::IpAddr;

#[derive(Debug, thiserror::Error)]
pub enum UrlError {
    #[error("Only http:// and https:// addresses are allowed.")]
    BadScheme,
    #[error("This looks like a local or private network address. If you really mean a local model server (Ollama, LM Studio…), enable “Allow local model endpoints” first.")]
    LocalBlocked,
    #[error("Could not understand this address.")]
    Unparseable,
}

/// Fixed Proto-Familiar origin — Advoco's whole purpose. Never user input.
pub fn pf_base_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

/// Validate the fixed PF origin (scheme check only — see module docs).
pub fn validate_pf_url(url: &str) -> Result<(), UrlError> {
    validate(url, true)
}

/// Validate a user-supplied LLM endpoint.
pub fn validate_llm_url(url: &str, allow_local: bool) -> Result<(), UrlError> {
    validate(url, allow_local)
}

fn validate(url: &str, allow_local: bool) -> Result<(), UrlError> {
    let parsed = url::Url::parse(url).map_err(|_| UrlError::Unparseable)?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => return Err(UrlError::BadScheme),
    }
    if allow_local {
        return Ok(());
    }
    let host = parsed.host_str().ok_or(UrlError::Unparseable)?;
    // Literal loopback names.
    if host.eq_ignore_ascii_case("localhost") || host.eq_ignore_ascii_case("local") {
        return Err(UrlError::LocalBlocked);
    }
    // IPv6 literals come back bracketed ("[::1]"); strip before parsing.
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    // Resolve and check the IP. If resolution fails, the request will fail
    // anyway; do not block on DNS here.
    if let Ok(addrs) = bare.parse::<IpAddr>() {
        if !is_public_ip(addrs) {
            return Err(UrlError::LocalBlocked);
        }
    }
    Ok(())
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.octets()[0] == 169 && v4.octets()[1] == 254 // link-local, belt & braces
                || v4.octets()[0] >= 224) // multicast + reserved
        }
        IpAddr::V6(v6) => {
            !(v6.is_loopback()
                || v6.is_unspecified()
                || (v6.segments()[0] & 0xfe00) == 0xfc00 // unique-local fc00::/7
                || (v6.segments()[0] & 0xffc0) == 0xfe80 // link-local fe80::/10
                || (v6.segments()[0] & 0xff00) == 0xff00) // multicast
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_http_schemes() {
        assert!(validate_llm_url("ftp://example.com", false).is_err());
        assert!(validate_llm_url("file:///etc/passwd", false).is_err());
        assert!(validate_llm_url("gopher://x", false).is_err());
    }

    #[test]
    fn rejects_private_and_loopback_by_default() {
        for url in [
            "http://localhost:11434/v1",
            "http://127.0.0.1:8080/v1",
            "http://10.0.0.5/v1",
            "http://192.168.1.10/v1",
            "http://172.16.0.1/v1",
            "http://169.254.1.1/v1",
            "http://0.0.0.0/v1",
            "http://[::1]:8080/v1",
            "http://[fe80::1]/v1",
        ] {
            assert!(validate_llm_url(url, false).is_err(), "should block {url}");
        }
    }

    #[test]
    fn allows_public_hosts() {
        for url in [
            "https://api.anthropic.com/v1/messages",
            "https://api.openai.com/v1/chat/completions",
            "https://openrouter.ai/api/v1/chat/completions",
            "https://my-llm.example.com/v1",
        ] {
            assert!(validate_llm_url(url, false).is_ok(), "should allow {url}");
        }
    }

    #[test]
    fn local_toggle_unlocks_loopback_for_llm() {
        assert!(validate_llm_url("http://127.0.0.1:11434/v1", true).is_ok());
        assert!(validate_llm_url("http://localhost:11434/v1", true).is_ok());
    }

    #[test]
    fn pf_constant_always_valid() {
        assert!(validate_pf_url(&pf_base_url(8742)).is_ok());
    }
}
