//! `web_fetch`'s document (`docs/tools/web-search.md` *`web_fetch`*): what a
//! body is, how its bytes become text, and how a long text becomes pages.
//!
//! Pure. The request, the address gate's resolution and the rescue through a
//! search source are the boundary's; deciding whether a body is HTML, decoding
//! it in the charset it declares, and cutting it into pages the model can ask
//! for one at a time is all here.

/// What a body is, from its media type — the only thing that decides whether
/// it is converted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Media {
    /// HTML: converted to markdown.
    Html,
    /// Plain text, markdown, JSON, XML, CSV: passed through.
    Text,
    /// An image, a PDF, an archive: refused **by name**, so the model learns
    /// what it was rather than reading an empty page.
    Binary(&'static str),
    /// Something the header did not describe; guessed from the bytes.
    Unknown,
}

/// Classify a `content-type` header value.
#[must_use]
pub fn media_from_content_type(content_type: &str) -> Media {
    let value = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if value.is_empty() {
        return Media::Unknown;
    }
    if value.contains("html") {
        return Media::Html;
    }
    if value.starts_with("text/")
        || value.contains("json")
        || value.contains("xml")
        || value.contains("javascript")
        || value.contains("x-yaml")
    {
        return Media::Text;
    }
    if value.starts_with("image/") {
        return Media::Binary("an image");
    }
    if value.contains("pdf") {
        return Media::Binary("a PDF");
    }
    if value.contains("zip")
        || value.contains("gzip")
        || value.contains("tar")
        || value.contains("rar")
        || value.contains("7z")
    {
        return Media::Binary("an archive");
    }
    if value.starts_with("audio/") || value.starts_with("video/") {
        return Media::Binary("audio or video");
    }
    Media::Unknown
}

/// The charset a body declares: the header's `charset=`, else an HTML
/// `<meta charset>` near the top, else UTF-8.
///
/// Measured on a docs page: 948 KB of HTML, and an old GBK page read as UTF-8
/// is mojibake — the fallback order is what makes one path for both.
#[must_use]
pub fn charset_of(content_type: &str, body: &[u8]) -> Charset {
    if let Some(charset) = content_type
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix("charset="))
    {
        return Charset::named(charset.trim_matches('"').trim());
    }
    // `<meta charset="gbk">` or `<meta http-equiv=… content="text/html;
    // charset=gbk">` in the first 4 KB — the head is always within that.
    let head = String::from_utf8_lossy(&body[..body.len().min(4096)]).to_ascii_lowercase();
    for needle in ["charset=", "<meta charset=\""] {
        if let Some(at) = head.find(needle) {
            let rest = &head[at + needle.len()..];
            let rest = rest.trim_start_matches(['"', '\'', ' ']);
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                .collect();
            if !name.is_empty() {
                return Charset::named(&name);
            }
        }
    }
    Charset::Utf8
}

/// A character set a body declares. Only the families this crate can decode
/// without a dependency are distinguished; anything else reads as UTF-8 with
/// replacement characters, which is the honest failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Charset {
    /// UTF-8.
    Utf8,
    /// Latin-1 and its Windows-1252 superset, byte-for-byte.
    Latin1,
    /// Anything else: decoded as UTF-8, replacement characters and all.
    Other,
}

impl Charset {
    /// The spelling in a header, as a value.
    #[must_use]
    pub fn named(name: &str) -> Charset {
        let name = name.to_ascii_lowercase();
        match name.as_str() {
            "utf-8" | "utf8" | "" => Charset::Utf8,
            "iso-8859-1" | "iso8859-1" | "latin1" | "latin-1" | "windows-1252" | "cp1252" => {
                Charset::Latin1
            }
            _ => Charset::Other,
        }
    }
}

/// Decode a body with its declared charset.
#[must_use]
pub fn decode(body: &[u8], charset: Charset) -> String {
    match charset {
        Charset::Utf8 | Charset::Other => String::from_utf8_lossy(body).into_owned(),
        Charset::Latin1 => body.iter().map(|byte| char::from(*byte)).collect(),
    }
}

/// Whether a body should be read as HTML when the header said nothing: it
/// starts with a tag, after any leading whitespace or doctype.
#[must_use]
pub fn looks_like_html(body: &str) -> bool {
    let head = body.trim_start();
    head.starts_with('<')
}

/// What one page of a document holds, and where the rest is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// This page's text.
    pub text: String,
    /// Which page this is (1-based).
    pub number: usize,
    /// How many pages the document has.
    pub total: usize,
}

impl Page {
    /// Whether the model should be told there is more.
    #[must_use]
    pub fn has_more(&self) -> bool {
        self.number < self.total
    }
}

/// Cut `text` into pages of at most `page_chars` characters, never splitting
/// one — the rule every cut in this crate follows.
///
/// A page is cut on a line boundary when one is available near the edge, so a
/// code block or a table row is not sliced in half; the search is bounded to
/// the last tenth of the page.
#[must_use]
pub fn pages(text: &str, page_chars: usize) -> Vec<Page> {
    let chars: Vec<char> = text.chars().collect();
    if page_chars == 0 || chars.is_empty() {
        return vec![Page {
            text: String::new(),
            number: 1,
            total: 1,
        }];
    }
    let mut bounds: Vec<usize> = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let mut end = (start + page_chars).min(chars.len());
        if end < chars.len() {
            // Prefer the last newline inside the page, but only in the page's
            // second half — a break in the first half would produce a run of
            // half-empty pages, which costs more requests than it saves.
            let floor = start + page_chars / 2;
            if let Some(at) = (start..end).rev().find(|&i| chars[i] == '\n')
                && at >= floor
            {
                end = at + 1;
            }
        }
        bounds.push(end);
        start = end;
    }
    let total = bounds.len();
    let mut out = Vec::with_capacity(total);
    let mut from = 0;
    for (index, end) in bounds.into_iter().enumerate() {
        out.push(Page {
            text: chars[from..end].iter().collect(),
            number: index + 1,
            total,
        });
        from = end;
    }
    out
}

/// The page the model asked for: `number` clamped into range, so a stale
/// request reads the last page rather than failing.
#[must_use]
pub fn page(document: &[Page], number: usize) -> &Page {
    let index = number
        .saturating_sub(1)
        .min(document.len().saturating_sub(1));
    &document[index]
}

#[cfg(test)]
mod tests;
