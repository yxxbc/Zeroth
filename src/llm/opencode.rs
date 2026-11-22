//! OpenCode Go's request identity — the two headers its gateway makes
//! load-bearing for any client but OpenCode itself.
//!
//! Go's inference API (`https://opencode.ai/inference/go/openai/v1`) is a
//! Chat Completions endpoint like any other, but a request from a client it
//! does not recognise is not merely slower: with a generic library
//! `User-Agent` Cloudflare answers `403 error code: 1010`, and with no
//! `x-opencode-session` the gateway answers `400 MissingSessionID` — "Request
//! is missing x-opencode-session and cannot be routed efficiently"
//! (`docs/providers/opencode.md`). The service's own instructions ask every
//! harness to identify itself by name and version and to send one stable
//! session id per conversation, which is what [`request_headers`] does.
//!
//! Pure: no I/O, no clock. The session key is minted at the boundary
//! (`tui::config::session_cache_key`) and arrives here as a string.

/// The per-conversation routing hint Go's gateway requires. Its absence is a
/// `400`, so this is a header a Go request cannot go without.
pub const SESSION_HEADER: &str = "x-opencode-session";

/// The path fragment that marks a **Go** base. The free OpenCode endpoint
/// (`/inference/openai/v1`) is a different backend; nothing here is sent to
/// it. A proxy that forwards to Go under the same path still gets the
/// headers, which is why the host is not part of the test.
const GO_PATH: &str = "/inference/go/";

/// What this client calls itself to Go, in the `name/version` shape the
/// service asks for (its docs' example is `my-coding-agent/1.0`). A generic
/// HTTP-library string is refused outright by the edge.
pub const USER_AGENT: &str = concat!("zeroth/", env!("CARGO_PKG_VERSION"));

/// Go's own usage page — where `/usage` links when a window runs out.
pub const DASHBOARD: &str = "https://opencode.ai/console/go";

/// Go's plan-usage endpoint for a chat base, or `None` when the base is not
/// Go's. The chat base sits under `…/inference/go/openai/v1`; the usage meter
/// is the sibling `…/inference/go/v1/usage` (`docs/providers/usage.md`). A
/// proxy that mounts the same path gets its own origin back, prefix included.
#[must_use]
pub fn usage_url(api_base: &str) -> Option<String> {
    let at = api_base.to_ascii_lowercase().find("/inference/go")?;
    let mut url = api_base[..at].trim_end_matches('/').to_string();
    if url.is_empty() {
        return None;
    }
    url.push_str("/inference/go/v1/usage");
    Some(url)
}

/// Is `api_base` OpenCode Go's inference base?
#[must_use]
pub fn is_go_base(api_base: &str) -> bool {
    api_base.to_ascii_lowercase().contains(GO_PATH)
}

/// The session header for a per-conversation key. No key means no header:
/// an empty session id is a *different* (invalid) routing hint than none,
/// the rule the ChatGPT and account-id headers already follow.
#[must_use]
pub fn session_headers(cache_key: Option<&str>) -> Vec<(String, String)> {
    cache_key
        .filter(|key| !key.is_empty())
        .map(|key| vec![(SESSION_HEADER.to_string(), key.to_string())])
        .unwrap_or_default()
}

/// Every header a Go request carries on top of its credential: this
/// client's `User-Agent` and the session id. Empty for every other base, so
/// an ordinary provider never sees a header it did not ask for.
#[must_use]
pub fn request_headers(api_base: &str, cache_key: Option<&str>) -> Vec<(String, String)> {
    if !is_go_base(api_base) {
        return Vec::new();
    }
    let mut headers = vec![("user-agent".to_string(), USER_AGENT.to_string())];
    headers.extend(session_headers(cache_key));
    headers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_go_base_is_the_inference_go_path() {
        assert!(is_go_base("https://opencode.ai/inference/go/openai/v1"));
        assert!(is_go_base("https://OpenCode.ai/inference/GO/openai/v1/"));
        // A company proxy that forwards the same path still gets the headers.
        assert!(is_go_base("http://proxy.example/inference/go/openai/v1"));
        // The free endpoint is a different backend, and a local router's
        // base is not Go's path at all.
        assert!(!is_go_base("https://opencode.ai/inference/openai/v1"));
        assert!(!is_go_base("http://127.0.0.1:3425/v1"));
        assert!(!is_go_base(""));
    }

    #[test]
    fn a_go_request_names_this_client_and_its_session() {
        assert_eq!(
            request_headers(
                "https://opencode.ai/inference/go/openai/v1",
                Some("zeroth-42")
            ),
            vec![
                ("user-agent".to_string(), USER_AGENT.to_string()),
                (SESSION_HEADER.to_string(), "zeroth-42".to_string()),
            ]
        );
        assert!(USER_AGENT.starts_with("zeroth/"), "{USER_AGENT}");
    }

    #[test]
    fn no_session_key_means_no_session_header() {
        // The service's `400 MissingSessionID` is about the header being
        // absent; an empty value would be a *different* routing hint, not a
        // usable one.
        assert_eq!(
            request_headers("https://opencode.ai/inference/go/openai/v1", None),
            vec![("user-agent".to_string(), USER_AGENT.to_string())]
        );
        assert!(session_headers(Some("")).is_empty());
    }

    #[test]
    fn the_usage_meter_is_the_sibling_of_the_chat_base() {
        assert_eq!(
            usage_url("https://opencode.ai/inference/go/openai/v1").as_deref(),
            Some("https://opencode.ai/inference/go/v1/usage")
        );
        // A proxy mount keeps its origin and prefix.
        assert_eq!(
            usage_url("http://proxy.example/mount/inference/go/openai/v1").as_deref(),
            Some("http://proxy.example/mount/inference/go/v1/usage")
        );
        assert_eq!(usage_url("https://opencode.ai/inference/openai/v1"), None);
        assert_eq!(usage_url("http://127.0.0.1:3425/v1"), None);
        assert_eq!(usage_url(""), None);
    }

    #[test]
    fn an_ordinary_provider_gets_no_opencode_headers() {
        // A header it never asked for is a header a strict shim can refuse.
        assert!(request_headers("https://api.openai.com/v1", Some("zeroth-42")).is_empty());
        assert!(request_headers("http://127.0.0.1:3425/v1", Some("zeroth-42")).is_empty());
    }
}
