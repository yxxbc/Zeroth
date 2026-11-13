//! Link preview cards — the I/O boundary (`docs/ui/rich-links.md`): fetch one
//! **lone** link's page, parse its metadata (`zeroth::link_card`) and hand the
//! card back to the event loop. A worker thread with a blocking client, the
//! clipboard / image-dimension pattern, so the screen never waits on a page.
//!
//! Policy: `http(s)` only; a 5 s timeout; `text/html` only; at most 512 KiB
//! read; no cookies or credentials; private/loopback/link-local hosts refused,
//! including on a redirect. The card cache lives beside the sessions under the
//! config home, so a card survives a restart and the same URL is never fetched
//! twice. `ALTER_ZERO_LINK_CARDS` (default on; a falsy value disables it) turns
//! the whole feature off — no request, no card.

use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

use zeroth::link_card::{LinkCard, is_private_host};

/// How long one page may take.
const TIMEOUT: Duration = Duration::from_secs(5);
/// How much HTML is read before parsing stops.
const BODY_CAP: u64 = 512 * 1024;
/// A redirect chain longer than this is refused.
const REDIRECTS: usize = 3;

/// Whether cards are enabled for this process (`ALTER_ZERO_LINK_CARDS`).
#[must_use]
pub(crate) fn enabled() -> bool {
    enabled_value(std::env::var("ALTER_ZERO_LINK_CARDS").ok().as_deref())
}

/// [`enabled`]'s pure half: a falsy value (`0`, `false`, `no`) disables;
/// anything else — including unset — leaves it on.
#[must_use]
pub(crate) fn enabled_value(value: Option<&str>) -> bool {
    !value.is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "no"
        )
    })
}

/// The cache file — `~/.zeroth/cache/link-cards.json` (the config home's
/// sibling of sessions and checkpoints).
fn cache_path() -> Option<PathBuf> {
    super::config::config_home().map(|home| home.join("cache").join("link-cards.json"))
}

/// Load the cached cards. A missing or unreadable file is an empty cache, not
/// an error: the feature simply fetches again.
#[must_use]
pub(crate) fn load() -> HashMap<String, LinkCard> {
    let Some(path) = cache_path() else {
        return HashMap::new();
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Persist the cache best-effort: a failure loses cards on the next start, not
/// the session.
pub(crate) fn save(cards: &HashMap<String, LinkCard>) {
    let Some(path) = cache_path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string(cards) {
        let _ = std::fs::write(path, text);
    }
}

/// Fetch `url` on its own thread and send `(url, card)` back — `None` for a
/// page with nothing to show or a refused/failed request. A result is always
/// sent, so the caller can clear its in-flight marker.
pub(crate) fn spawn(
    url: String,
    tx: tokio::sync::mpsc::UnboundedSender<(String, Option<LinkCard>)>,
) {
    std::thread::spawn(move || {
        let card = fetch(&url);
        let _ = tx.send((url, card));
    });
}

/// One fetch under the policy above.
fn fetch(url: &str) -> Option<LinkCard> {
    let parsed = reqwest::Url::parse(url).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    if parsed.host_str().is_none_or(is_private_host) {
        return None;
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(TIMEOUT)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= REDIRECTS
                || attempt.url().host_str().is_none_or(is_private_host)
            {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .user_agent(concat!(
            env!("CARGO_PKG_NAME"),
            "/",
            env!("CARGO_PKG_VERSION"),
            " (link preview)"
        ))
        .build()
        .ok()?;
    let response = client.get(parsed).send().ok()?;
    if !response.status().is_success() {
        return None;
    }
    let html = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())?;
    if !html.to_ascii_lowercase().contains("text/html") {
        return None;
    }
    let mut body = String::new();
    response.take(BODY_CAP).read_to_string(&mut body).ok()?;
    zeroth::link_card::parse(&body, url)
}

#[cfg(test)]
mod tests {
    use super::enabled_value;

    #[test]
    fn link_cards_are_on_unless_explicitly_disabled() {
        assert!(enabled_value(None));
        assert!(enabled_value(Some("1")));
        assert!(enabled_value(Some("on")));
        assert!(!enabled_value(Some("0")));
        assert!(!enabled_value(Some("false")));
        assert!(!enabled_value(Some(" NO ")));
    }
}
