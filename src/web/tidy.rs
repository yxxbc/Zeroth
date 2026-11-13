//! Tidying a source's hits into what the model reads (`docs/tools/web-search.md`
//! *The walk*): the domain filter, deduplication, the excerpt cap, the result
//! count.
//!
//! Three details are load-bearing and each has its reason recorded:
//!
//! - **The domain match is on labels, not substrings.** `example.com` must
//!   match `docs.example.com` and must not match `notexample.com` — the
//!   difference between a filter and an attacker's best friend.
//! - **Pushing the filter upstream doubles the ask.** A source with
//!   `site_operator` gets the allowed domains written into its query, but the
//!   local filter still runs (the source may ignore it, or return docs
//!   subdomains the query did not name), so a filtered search asks for
//!   `results * 2` and the count is applied after filtering.
//! - **The excerpt is cut on a character boundary, not a byte.** A 1,200-char
//!   cap that slices a multi-byte character produces mojibake in the cell and
//!   invalid UTF-8 on the wire.

use crate::web::sources::Hit;

/// What to keep: the caller's query, as the filter sees it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    /// Keep only these domains (and their subdomains). Empty keeps all.
    pub allowed: Vec<String>,
    /// Drop these domains (and their subdomains).
    pub blocked: Vec<String>,
}

impl Filter {
    /// Whether anything is being filtered at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.allowed.is_empty() && self.blocked.is_empty()
    }

    /// Normalize a domain written by a model or a user: accept
    /// `https://www.example.com/x`, `example.com`, `www.example.com`, and
    /// reduce each to the host it names.
    #[must_use]
    pub fn normalize(domain: &str) -> String {
        let mut text = domain.trim().to_ascii_lowercase();
        if let Some((_, rest)) = text.split_once("://") {
            text = rest.to_string();
        }
        // Drop the path, the query, the fragment and any port.
        let text = text.split(['/', '?', '#']).next().unwrap_or("").to_string();
        let text = text.rsplit('@').next().unwrap_or(&text).to_string();
        let text = text
            .rsplit_once(':')
            .filter(|(_, port)| !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()))
            .map_or(text.clone(), |(host, _)| host.to_string());
        // `www.` is stripped on **both** sides — here and in [`host_of`] — so a
        // user who writes `www.example.com` and a hit that is `example.com`
        // (or the reverse) still match. Otherwise the two spellings of one
        // site are two different filters.
        let text = text.trim_matches('.').to_ascii_lowercase();
        text.strip_prefix("www.").unwrap_or(&text).to_string()
    }

    /// Whether `url`'s host passes. A URL that cannot be read has no host, so
    /// it passes an allow-list only if the list is empty — an unreadable hit
    /// is not evidence of a matching domain.
    #[must_use]
    pub fn admits(&self, url: &str) -> bool {
        let host = host_of(url);
        if let Some(host) = &host
            && self.blocked.iter().any(|blocked| under(host, blocked))
        {
            return false;
        }
        if self.allowed.is_empty() {
            return true;
        }
        host.is_some_and(|host| self.allowed.iter().any(|allowed| under(&host, allowed)))
    }
}

/// Whether `host` is `domain` or a subdomain of it — by labels, so
/// `notexample.com` is not under `example.com` and `example.com.evil.com` is
/// not either.
fn under(host: &str, domain: &str) -> bool {
    if domain.is_empty() {
        return false;
    }
    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|prefix| prefix.ends_with('.'))
}

/// The host of a URL, lowercased, with `www.` removed — or `None` when the
/// URL cannot be read.
#[must_use]
pub fn host_of(url: &str) -> Option<String> {
    let (_, rest) = url.split_once("://")?;
    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .rsplit('@')
        .next()
        .unwrap_or("");
    let host = if let Some(stripped) = authority.strip_prefix('[') {
        stripped.split_once(']').map(|(inside, _)| inside)?
    } else {
        authority.split(':').next().unwrap_or("")
    };
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    Some(host.strip_prefix("www.").unwrap_or(&host).to_string())
}

/// Tidy `hits` into what the model reads: filter, dedupe, cap the excerpt,
/// cut to `most`.
///
/// The order matters — filter before the count, so a domain-scoped search
/// still returns `most` results when it can.
#[must_use]
pub fn tidy(mut hits: Vec<Hit>, filter: &Filter, most: usize, excerpt_chars: usize) -> Vec<Hit> {
    hits.retain(|hit| !hit.url.is_empty() && filter.admits(&hit.url));

    // Dedupe on the URL without its fragment and with no trailing slash: two
    // providers routinely return the same page twice under `#anchor` forms.
    let mut seen = std::collections::HashSet::new();
    hits.retain(|hit| seen.insert(canonical_url(&hit.url)));

    for hit in &mut hits {
        if let Some(snippet) = hit.snippet.take() {
            let cut = cut_chars(&snippet, excerpt_chars);
            if !cut.trim().is_empty() {
                hit.snippet = Some(cut);
            }
        }
    }
    hits.truncate(most);
    hits
}

/// A URL reduced to what identifies the page: no fragment, no trailing slash,
/// lowercase host.
fn canonical_url(url: &str) -> String {
    let without_fragment = url.split('#').next().unwrap_or(url);
    let trimmed = without_fragment.trim_end_matches('/');
    match trimmed.split_once("://") {
        Some((scheme, rest)) => match rest.split_once('/') {
            Some((authority, path)) => {
                format!(
                    "{}://{}/{}",
                    scheme.to_ascii_lowercase(),
                    authority.to_ascii_lowercase(),
                    path
                )
            }
            None => format!(
                "{}://{}",
                scheme.to_ascii_lowercase(),
                rest.to_ascii_lowercase()
            ),
        },
        None => trimmed.to_string(),
    }
}

/// At most `most` characters, never splitting one.
///
/// A cut is marked with `…` so the model can tell an excerpt from a short
/// page — and the marker is inside the budget, not added to it.
#[must_use]
pub fn cut_chars(text: &str, most: usize) -> String {
    if most == 0 {
        return String::new();
    }
    if text.chars().count() <= most {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(most.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

#[cfg(test)]
mod tests;
