//! Reading a scraped result page (`docs/tools/web-search.md` *Measured* §1).
//!
//! Only Bing ships. DuckDuckGo answered a **202 anomaly page** to a plain
//! request — the same IP reputation problem that killed the `jina_web_search`
//! script — and was measured out, so there is nothing to parse.
//!
//! Bing's markup has no plain `href`: every result link is a `/ck/a?u=a1…`
//! redirect carrying the target base64url-encoded
//! ([`crate::web::guard::unwrap_bing_redirect`]). A hit whose link cannot be
//! unwrapped is dropped rather than shown as a Bing URL, because a result the
//! model cannot open is worse than one fewer result.

use crate::web::guard::unwrap_bing_redirect;
use crate::web::sources::Hit;

/// Parse a Bing result page.
#[must_use]
pub fn bing(html: &str) -> Vec<Hit> {
    let mut hits = Vec::new();
    for block in bing_blocks(html) {
        let Some(href) = first_href(block) else {
            continue;
        };
        let url = unwrap_bing_redirect(&href);
        // Still a Bing redirect after unwrapping means the payload did not
        // decode; a URL the model cannot open is worse than one fewer hit.
        if !url.starts_with("http") || is_bing_redirect(&url) {
            continue;
        }
        let title = strip_tags(first_in_tag(block, "h2").unwrap_or_default());
        let snippet = strip_tags(caption(block));
        hits.push(Hit {
            title: title.trim().to_string(),
            url,
            snippet: Some(snippet.trim().to_string()).filter(|s| !s.is_empty()),
            date: None,
        });
    }
    hits
}

/// The page split at each result block. Bing wraps every organic result in
/// `<li class="b_algo">` — measured: ten of them on a live page.
fn bing_blocks(html: &str) -> Vec<&str> {
    const MARK: &str = "b_algo";
    // Collect every start once, then slice between neighbours: walking the
    // page twice per block is how the first draft of this went wrong.
    let mut starts = Vec::new();
    let mut from = 0;
    while let Some(found) = html[from..].find(MARK) {
        let at = from + found;
        starts.push(html[..at].rfind("<li").unwrap_or(at));
        from = at + MARK.len();
    }
    (0..starts.len())
        .map(|n| {
            let start = starts[n];
            let end = starts.get(n + 1).copied().unwrap_or(html.len());
            &html[start..end]
        })
        .collect()
}

/// The first `href` in a fragment.
fn first_href(block: &str) -> Option<String> {
    let at = block.find("href=")?;
    let rest = &block[at + 5..];
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let body = &rest[1..];
    let end = body.find(quote)?;
    Some(body[..end].to_string())
}

/// The contents of the first `<tag …>` element.
fn first_in_tag<'a>(block: &'a str, tag: &str) -> Option<&'a str> {
    let open = block.find(&format!("<{tag}"))?;
    let start = block[open..].find('>')? + open + 1;
    let close = block[start..].find(&format!("</{tag}>"))? + start;
    Some(&block[start..close])
}

/// Whether `url` is one of Bing's own `/ck/a` redirects — what an undecoded
/// payload looks like.
fn is_bing_redirect(url: &str) -> bool {
    url.contains("bing.com/ck/a")
}

/// A result's snippet: everything inside `<div class="b_caption">`, tags
/// already stripped. Taking text up to the next `<` stops at the `<p>` and
/// yields nothing, which is how the first version dropped every snippet.
fn caption(block: &str) -> &str {
    let Some(at) = block.find("b_caption") else {
        return "";
    };
    let Some(open) = block[at..].find('>').map(|offset| at + offset + 1) else {
        return "";
    };
    let end = block[open..]
        .find("</div>")
        .map_or(block.len(), |offset| open + offset);
    &block[open..end]
}

/// Drop tags and collapse entities, so a title reads as text.
fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut inside = false;
    for c in html.chars() {
        match c {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

#[cfg(test)]
mod tests;
