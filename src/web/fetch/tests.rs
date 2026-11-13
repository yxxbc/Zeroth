//! The document rules: media type, charset, and the paging that makes a
//! docs.rs page readable one page at a time (`docs/tools/web-search.md`
//! *Measured* §6: 220,142 characters of text on that page).

use super::*;

#[test]
fn the_media_type_decides_what_is_converted() {
    assert_eq!(
        media_from_content_type("text/html; charset=utf-8"),
        Media::Html
    );
    assert_eq!(
        media_from_content_type("application/xhtml+xml"),
        Media::Html
    );
    assert_eq!(media_from_content_type("text/plain"), Media::Text);
    assert_eq!(media_from_content_type("application/json"), Media::Text);
    assert_eq!(media_from_content_type("text/markdown"), Media::Text);
    assert_eq!(media_from_content_type("application/xml"), Media::Text);
}

#[test]
fn a_binary_is_refused_by_name_so_the_model_learns_what_it_was() {
    // Refusing silently would hand back an empty page and let the model
    // conclude the page was blank.
    assert_eq!(
        media_from_content_type("image/png"),
        Media::Binary("an image")
    );
    assert_eq!(
        media_from_content_type("application/pdf"),
        Media::Binary("a PDF")
    );
    assert_eq!(
        media_from_content_type("application/zip"),
        Media::Binary("an archive")
    );
    assert_eq!(
        media_from_content_type("video/mp4"),
        Media::Binary("audio or video")
    );
}

#[test]
fn an_unstated_type_is_unknown_rather_than_assumed() {
    assert_eq!(media_from_content_type(""), Media::Unknown);
    assert_eq!(
        media_from_content_type("application/octet-stream"),
        Media::Unknown
    );
}

#[test]
fn html_is_recognised_from_the_bytes_when_the_header_says_nothing() {
    assert!(looks_like_html("<!DOCTYPE html><html>"));
    assert!(looks_like_html("  \n<html>"));
    assert!(!looks_like_html("{\"json\": true}"));
    assert!(!looks_like_html("plain words"));
}

// ===== charset =====

#[test]
fn the_header_charset_wins() {
    assert_eq!(
        charset_of("text/html; charset=iso-8859-1", b"<html>"),
        Charset::Latin1
    );
    assert_eq!(
        charset_of("text/html; charset=\"utf-8\"", b""),
        Charset::Utf8
    );
}

#[test]
fn a_declared_meta_charset_is_honoured_when_the_header_is_silent() {
    // An old GBK page read as UTF-8 is mojibake, and this is where that is
    // decided.
    let body = br#"<html><head><meta charset="gbk"></head>"#;
    assert_eq!(charset_of("text/html", body), Charset::Other);
}

#[test]
fn an_http_equiv_meta_charset_is_read_too() {
    let body =
        br#"<html><head><meta http-equiv="Content-Type" content="text/html; charset=windows-1252">"#;
    assert_eq!(charset_of("text/html", body), Charset::Latin1);
}

#[test]
fn nothing_declared_reads_as_utf8() {
    assert_eq!(charset_of("text/html", b"<html><body>hi"), Charset::Utf8);
    assert_eq!(charset_of("", b""), Charset::Utf8);
}

#[test]
fn a_latin1_body_decodes_byte_for_byte() {
    // 0xe9 is 'é' in Latin-1 and invalid alone in UTF-8.
    assert_eq!(decode(&[0x63, 0x61, 0x66, 0xe9], Charset::Latin1), "café");
}

#[test]
fn an_undecodable_utf8_body_keeps_going_with_replacement_characters() {
    let text = decode(&[0x61, 0xff, 0x62], Charset::Utf8);
    assert!(text.starts_with('a') && text.ends_with('b'), "{text}");
    assert!(text.contains('\u{FFFD}'), "{text}");
}

#[test]
fn an_unknown_charset_does_not_fail_the_page() {
    let text = decode(b"plain ascii", Charset::Other);
    assert_eq!(text, "plain ascii");
}

// ===== pages =====

#[test]
fn a_short_document_is_one_page() {
    let document = pages("short", 100);
    assert_eq!(document.len(), 1);
    assert_eq!(document[0].number, 1);
    assert_eq!(document[0].total, 1);
    assert!(!document[0].has_more());
}

#[test]
fn a_long_document_is_cut_into_pages_of_the_cap() {
    let text = "x".repeat(250);
    let document = pages(&text, 100);
    assert_eq!(document.len(), 3);
    assert_eq!(document[0].total, 3);
    assert!(document[0].has_more());
    assert!(!document[2].has_more());
    assert_eq!(
        document
            .iter()
            .map(|p| p.text.chars().count())
            .sum::<usize>(),
        250,
        "no character is lost at a boundary"
    );
}

#[test]
fn pages_are_cut_on_a_line_boundary_when_one_is_near_the_edge() {
    // A table row or a code line sliced in half is unreadable twice.
    let line = "a".repeat(40);
    let text = vec![line.clone(); 10].join("\n");
    let document = pages(&text, 100);
    for page in &document {
        assert!(
            page.text.ends_with('\n') || page.number == page.total,
            "page {} ends mid-line: {:?}",
            page.number,
            &page.text[page.text.len().saturating_sub(10)..]
        );
    }
}

#[test]
fn a_multibyte_document_is_never_cut_inside_a_character() {
    let text = "中文".repeat(300);
    let document = pages(&text, 50);
    for page in &document {
        assert_eq!(page.text.chars().count(), 50);
        assert!(std::str::from_utf8(page.text.as_bytes()).is_ok());
    }
}

#[test]
fn asking_past_the_end_reads_the_last_page_rather_than_failing() {
    let document = pages(&"x".repeat(250), 100);
    assert_eq!(page(&document, 1).number, 1);
    assert_eq!(page(&document, 3).number, 3);
    assert_eq!(page(&document, 99).number, 3, "clamped, not an error");
    assert_eq!(page(&document, 0).number, 1);
}

#[test]
fn an_empty_document_is_one_empty_page() {
    let document = pages("", 100);
    assert_eq!(document.len(), 1);
    assert_eq!(document[0].text, "");
}
