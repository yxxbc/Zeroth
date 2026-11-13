//! Link preview metadata — the pure half of the full-screen mode's link cards
//! (`docs/ui/rich-links.md`). A **lone** URL line (the model shares a link on
//! its own line) gets a small card: the site, the page title and its
//! description, fetched by the boundary and cached. Parsing and policy are
//! here so they are unit-tested without a network.
//!
//! Nothing in this module performs I/O. `fetch` is the boundary's
//! (`tui::link_cards`): it uses a 5 s timeout, a 512 KiB body cap,
//! `text/html` only, no cookies or credentials, and refuses private/loopback
//! hosts (model output is untrusted, and a preview must not become a probe of
//! the user's LAN). Redirects are re-checked the same way.

use serde::{Deserialize, Serialize};

/// The compact card shown under a lone link: where it goes, what it is called,
/// and what it says it is about. All three trimmed; `description` may be
/// empty.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkCard {
    /// The page title (`og:title`, else `<title>`).
    pub title: String,
    /// The site's name (`og:site_name`, else the URL host without `www.`).
    pub site: String,
    /// `og:description`, else `<meta name="description">`; empty when neither.
    #[serde(default)]
    pub description: String,
    /// The page's `og:image`/`twitter:image`, verbatim; the boundary resolves
    /// it against the page URL, downloads it and fills [`cover`](Self::cover).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover_url: Option<String>,
    /// The downloaded cover thumbnail (`docs/ui/rich-links.md`): a cached file
    /// and its pixel size, so the text card can grow a picture without another
    /// metadata parse.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<Cover>,
}

/// A cached cover image: where the boundary saved it and how large it is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cover {
    /// The file's **absolute** path under the card cache, so the image store
    /// can read it at paint time on this machine.
    pub path: String,
    /// The image's pixel size, for the cell fit.
    pub px: (u32, u32),
}

impl LinkCard {
    /// Whether there is enough for a card worth drawing: a title. The site is
    /// the host when the page names none.
    #[must_use]
    pub fn is_meaningful(&self) -> bool {
        !self.title.is_empty()
    }
}

/// Build a card from a page's HTML and the URL that served it. `None` when the
/// page yielded neither a title nor a site.
#[must_use]
pub fn parse(html: &str, url: &str) -> Option<LinkCard> {
    let title = meta(html, "og:title")
        .or_else(|| meta(html, "twitter:title"))
        .or_else(|| title_tag(html))
        .unwrap_or_default();
    let site = meta(html, "og:site_name")
        .or_else(|| host_of(url))
        .unwrap_or_default();
    let description = meta(html, "og:description")
        .or_else(|| meta(html, "twitter:description"))
        .or_else(|| meta(html, "description"))
        .unwrap_or_default();
    let card = LinkCard {
        title: clean(&title),
        site: clean(&site),
        description: clean(&description),
        cover_url: meta(html, "og:image")
            .or_else(|| meta(html, "twitter:image"))
            .map(|url| url.trim().to_string())
            .filter(|url| !url.is_empty()),
        cover: None,
    };
    card.is_meaningful().then_some(card)
}

/// The URL's host without a leading `www.`; empty when it has none. Pure
/// string parsing — the URL came from the model, so no library panic is worth
/// risking on a malformed one (`reqwest::Url::parse` is fine with an https
/// URL, but callers may pass `file://` or prose).
#[must_use]
pub fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host_port = authority.rsplit('@').next()?;
    let host = if let Some(bracketed) = host_port.strip_prefix('[') {
        bracketed.split(']').next()?
    } else {
        host_port.split(':').next()?
    }
    .trim();
    if host.is_empty() {
        return None;
    }
    Some(host.strip_prefix("www.").unwrap_or(host).to_string())
}

/// Whether a host must never be fetched: loopback, link-local, private IPv4
/// ranges, `.local` names and the plain names a LAN resolver answers.
#[must_use]
pub fn is_private_host(host: &str) -> bool {
    let host = host.trim_matches(['[', ']']).to_ascii_lowercase();
    if host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host == "0.0.0.0"
        || host == "::1"
        || host == "::"
    {
        return true;
    }
    if let Some(rest) = host.strip_prefix("127.") {
        return rest.split('.').all(|part| part.parse::<u8>().is_ok());
    }
    if let Some(rest) = host.strip_prefix("10.") {
        return rest.split('.').all(|part| part.parse::<u8>().is_ok());
    }
    if let Some(rest) = host.strip_prefix("192.168.") {
        return rest.split('.').all(|part| part.parse::<u8>().is_ok());
    }
    if let Some(rest) = host.strip_prefix("172.")
        && let Some((second, rest)) = rest.split_once('.')
        && let Ok(second) = second.parse::<u8>()
        && (16..=31).contains(&second)
    {
        return rest.split('.').all(|part| part.parse::<u8>().is_ok());
    }
    if let Some(rest) = host.strip_prefix("169.254.") {
        return rest.split('.').all(|part| part.parse::<u8>().is_ok());
    }
    false
}

/// Find a `<meta property="key" content="…">` (or `name="key"`); the tag's
/// attributes may come in either order.
fn meta(html: &str, key: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    for attribute in ["property", "name"] {
        for quote in ['"', '\''] {
            let needle = format!("{attribute}={quote}{key}{quote}");
            let mut from = 0;
            while let Some(at) = lower[from..].find(&needle) {
                let start = from + at;
                // The attribute may sit anywhere in the tag; slice the whole
                // `<meta …>` so `content` is found whichever order it came in.
                let tag_start = lower[..start].rfind('<').unwrap_or(start);
                let end = lower[start..]
                    .find('>')
                    .map_or(lower.len(), |end| start + end);
                let tag = &html[tag_start..end];
                if let Some(content) = attribute_value(tag, "content") {
                    return Some(content);
                }
                from = end.max(start + needle.len());
            }
        }
    }
    None
}

/// The `<title>` element's text.
fn title_tag(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<title")?;
    let open_end = lower[start..].find('>')? + start + 1;
    let close = lower[open_end..].find("</title>")? + open_end;
    Some(html[open_end..close].to_string())
}

/// One attribute's value from a tag, in either quote style.
fn attribute_value(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    for quote in ['"', '\''] {
        let needle = format!("{name}={quote}");
        if let Some(at) = lower.find(&needle) {
            let start = at + needle.len();
            if let Some(end) = lower[start..].find(quote) {
                return Some(decode(&tag[start..start + end]));
            }
        }
    }
    None
}

/// The handful of entities a title/description realistically carries.
fn decode(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
}

/// Collapse whitespace and drop control characters — the text becomes one
/// terminal row (`docs/ui/rich-links.md`).
fn clean(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_open_graph_before_the_document_title() {
        let html = r#"
            <html><head>
            <title>Fallback title</title>
            <meta name="description" content="A plain description">
            <meta property="og:title" content="The real &amp; title">
            <meta property="og:site_name" content="Example">
            <meta property="og:description" content="What it is about">
            </head></html>"#;
        let card = parse(html, "https://www.example.com/post").expect("a card");
        assert_eq!(card.title, "The real & title");
        assert_eq!(card.site, "Example");
        assert_eq!(card.description, "What it is about");
    }

    #[test]
    fn falls_back_to_the_title_tag_and_the_host() {
        let card = parse(
            "<title>Hello</title><meta name='description' content='x'>",
            "https://www.example.com/a",
        )
        .expect("a card");
        assert_eq!(card.title, "Hello");
        assert_eq!(card.site, "example.com");
        assert_eq!(card.description, "x");
    }

    #[test]
    fn attributes_in_either_order_and_quote_style() {
        let card = parse(
            "<meta content=\"First\" property=\"og:title\">",
            "https://a.test",
        )
        .expect("a card");
        assert_eq!(card.title, "First");
        assert_eq!(card.site, "a.test");
    }

    #[test]
    fn a_page_with_no_title_or_site_is_no_card() {
        assert!(parse("<html><body>hi</body></html>", "https://a.test").is_none());
    }

    #[test]
    fn private_hosts_are_refused() {
        for host in [
            "localhost",
            "app.localhost",
            "printer.local",
            "127.0.0.1",
            "10.1.2.3",
            "192.168.0.1",
            "172.16.5.4",
            "169.254.1.1",
            "[::1]",
        ] {
            assert!(is_private_host(host), "{host} is private");
        }
        for host in ["example.com", "github.com", "172.32.1.1", "8.8.8.8"] {
            assert!(!is_private_host(host), "{host} is public");
        }
    }

    #[test]
    fn hosts_drop_www_and_ports() {
        assert_eq!(
            host_of("https://www.example.com:8443/x").as_deref(),
            Some("example.com")
        );
        assert_eq!(host_of("http://a.test").as_deref(), Some("a.test"));
        assert_eq!(host_of("not a url"), None);
    }
}
