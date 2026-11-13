//! Link preview cards — the I/O boundary (`docs/ui/rich-links.md`): fetch one
//! **lone** link's page, parse its metadata (`zeroth::link_card`) and hand the
//! card back to the event loop. A worker thread with a blocking client, the
//! clipboard / image-dimension pattern, so the screen never waits on a page.
//!
//! Policy: `http(s)` only; a 5 s timeout; `text/html` only; at most 512 KiB
//! read; no cookies or credentials; private/loopback/link-local hosts refused,
//! including on a redirect. The page's `og:image` cover is downloaded under
//! the same host/redirect policy (2 MiB, `image/*`) into the card cache, so a
//! card survives a restart and the same URL is never fetched twice.
//! `ZEROTH_LINK_CARDS` (default on; a falsy value disables it) turns the
//! whole feature off — no request, no card.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use zeroth::link_card::{Cover, LinkCard, host_is_private};

/// How long one page may take.
const TIMEOUT: Duration = Duration::from_secs(5);
/// How much HTML is read before parsing stops.
const BODY_CAP: u64 = 512 * 1024;
/// How much of a cover image is read.
const COVER_CAP: u64 = 2 * 1024 * 1024;
/// The cover cache's total budget; oldest files go past it.
const COVER_CACHE_BYTES: u64 = 32 * 1024 * 1024;
/// A redirect chain longer than this is refused.
const REDIRECTS: usize = 3;

/// Whether cards are enabled for this process (`ZEROTH_LINK_CARDS`).
#[must_use]
pub(crate) fn enabled() -> bool {
    enabled_value(std::env::var("ZEROTH_LINK_CARDS").ok().as_deref())
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
    if parsed.host_str().is_none_or(host_is_private) {
        return None;
    }
    let client = client()?;
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
    let mut card = zeroth::link_card::parse(&body, url)?;
    // The cover is best-effort: no image leaves the text card intact.
    if let Some(image) = card.cover_url.take() {
        attach_cover(&mut card, &image, url);
    }
    Some(card)
}

/// The blocking client both requests share: timeout, no cookies, a generic
/// user agent, and a redirect policy that stops at the cap or at a private
/// host (`docs/ui/rich-links.md`).
fn client() -> Option<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(TIMEOUT)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= REDIRECTS
                || attempt.url().host_str().is_none_or(host_is_private)
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
        .ok()
}

/// Download `image` (relative to `page`), save it under the card cache and
/// record its path/size on `card`. The image is untrusted too: same host
/// policy, `image/*` only, 2 MiB cap, and a file that does not decode as an
/// image is removed rather than placed.
fn attach_cover(card: &mut LinkCard, image: &str, page: &str) {
    let Some(dir) = cache_path().and_then(|path| path.parent().map(Path::to_path_buf)) else {
        return;
    };
    let Some(url) = reqwest::Url::parse(page)
        .ok()
        .and_then(|base| base.join(image).ok())
    else {
        return;
    };
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none_or(host_is_private) {
        return;
    }
    let Some(client) = client() else {
        return;
    };
    let Ok(response) = client.get(url.clone()).send() else {
        return;
    };
    if !response.status().is_success() {
        return;
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !content_type.starts_with("image/") {
        return;
    }
    let mut bytes = Vec::new();
    if response.take(COVER_CAP).read_to_end(&mut bytes).is_err() {
        return;
    }
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let mut hasher = DefaultHasher::new();
    url.as_str().hash(&mut hasher);
    let path = dir.join(format!("{:016x}.img", hasher.finish()));
    if std::fs::write(&path, &bytes).is_err() {
        return;
    }
    let px = image::ImageReader::open(&path)
        .ok()
        .and_then(|reader| reader.with_guessed_format().ok())
        .and_then(|reader| reader.into_dimensions().ok());
    let Some(px) = px else {
        let _ = std::fs::remove_file(&path);
        return;
    };
    card.cover = Some(Cover {
        path: path.display().to_string(),
        px,
    });
    prune_covers(&dir);
}

/// Keep the cover cache under [`COVER_CACHE_BYTES`], oldest first. Best
/// effort: an unreadable directory or file simply ends the sweep.
fn prune_covers(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "img") {
                return None;
            }
            let meta = entry.metadata().ok()?;
            Some((meta.modified().ok()?, meta.len(), path))
        })
        .collect();
    let mut total: u64 = files.iter().map(|(_, size, _)| *size).sum();
    if total <= COVER_CACHE_BYTES {
        return;
    }
    files.sort_by_key(|(modified, _, _)| *modified);
    for (_, size, path) in files {
        if total <= COVER_CACHE_BYTES {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(size);
        }
    }
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
