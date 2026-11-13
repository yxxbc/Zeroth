//! Bing's result markup, against the shape measured on a live page
//! (`docs/tools/web-search.md` *Measured* §1: ten `b_algo` blocks, every link
//! a `/ck/a?u=a1…` redirect).

use super::*;

/// base64url of a URL, as Bing encodes it.
fn enc(url: &str) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let bytes = url.as_bytes();
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        for i in 0..chunk.len() + 1 {
            out.push(TABLE[(n >> (18 - 6 * i) & 0x3f) as usize] as char);
        }
    }
    out
}

fn result(url: &str, title: &str, snippet: &str) -> String {
    format!(
        r#"<li class="b_algo"><h2><a href="https://www.bing.com/ck/a?!&amp;&amp;p=abc&amp;u=a1{}&amp;ntb=1">{title}</a></h2><div class="b_caption"><p>{snippet}</p></div></li>"#,
        enc(url)
    )
}

#[test]
fn a_result_becomes_a_hit_with_a_real_url() {
    let html = result("https://rustify.rs/a", "Rust Async", "A comparison");
    let hits = bing(&html);
    assert_eq!(hits.len(), 1);
    assert_eq!(
        hits[0].url, "https://rustify.rs/a",
        "the redirect is unwrapped"
    );
    assert_eq!(hits[0].title, "Rust Async");
    assert_eq!(hits[0].snippet.as_deref(), Some("A comparison"));
}

#[test]
fn several_blocks_are_read_in_order() {
    let html = [
        result("https://a.test/1", "One", "first"),
        result("https://b.test/2", "Two", "second"),
    ]
    .concat();
    let hits = bing(&html);
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].url, "https://a.test/1");
    assert_eq!(hits[1].url, "https://b.test/2");
}

#[test]
fn a_link_that_cannot_be_unwrapped_is_dropped() {
    let html =
        r#"<li class="b_algo"><h2><a href="https://www.bing.com/ck/a?u=BROKEN">T</a></h2></li>"#;
    assert!(bing(html).is_empty());
}

#[test]
fn a_bot_check_page_yields_nothing_rather_than_garbage() {
    let html = "<html><body><div class=\"anomaly\">unusual traffic</div></body></html>";
    assert!(bing(html).is_empty());
}

#[test]
fn entities_and_tags_are_cleaned_out_of_text() {
    let html = result("https://a.test/1", "A &amp; B", "x &lt;y&gt; <b>bold</b>");
    let hits = bing(&html);
    assert_eq!(hits[0].title, "A & B");
    assert_eq!(hits[0].snippet.as_deref(), Some("x <y> bold"));
}

#[test]
fn an_empty_snippet_is_none_not_an_empty_string() {
    let html = result("https://a.test/1", "T", "");
    assert_eq!(bing(&html)[0].snippet, None);
}

#[test]
fn a_page_with_no_results_is_empty() {
    assert!(bing("").is_empty());
    assert!(bing("<html><body>nothing</body></html>").is_empty());
}
