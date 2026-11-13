//! The address gate: every non-global range, and the Bing redirect decoder.

use super::*;
use std::net::Ipv4Addr;

fn v4(text: &str) -> IpAddr {
    IpAddr::V4(text.parse::<Ipv4Addr>().expect("a v4 literal"))
}

fn v6(text: &str) -> IpAddr {
    IpAddr::V6(text.parse().expect("a v6 literal"))
}

// ===== the classes =====

#[test]
fn loopback_is_not_global() {
    for address in ["127.0.0.1", "127.1.2.3", "127.255.255.254"] {
        assert_eq!(classify(v4(address)), AddressClass::Loopback, "{address}");
        assert!(!is_global(v4(address)));
    }
    assert_eq!(classify(v6("::1")), AddressClass::Loopback);
}

#[test]
fn the_metadata_endpoint_is_link_local() {
    // The one that matters most: cloud instance metadata.
    assert_eq!(classify(v4("169.254.169.254")), AddressClass::LinkLocal);
    assert_eq!(classify(v4("169.254.0.1")), AddressClass::LinkLocal);
    assert_eq!(classify(v6("fe80::1")), AddressClass::LinkLocal);
}

#[test]
fn private_ranges_are_private() {
    for address in [
        "10.0.0.1",
        "10.255.255.255",
        "172.16.0.1",
        "172.31.255.254",
        "192.168.1.1",
    ] {
        assert_eq!(classify(v4(address)), AddressClass::Private, "{address}");
    }
    assert_eq!(classify(v6("fd00::1")), AddressClass::Private);
    assert_eq!(classify(v6("fc00::1")), AddressClass::Private);
}

#[test]
fn the_not_routable_bin_is_special() {
    for address in [
        "0.0.0.0",
        "255.255.255.255",
        "224.0.0.1",
        "192.0.2.1",
        "198.51.100.1",
        "203.0.113.1",
        "100.64.0.1",
        "198.18.0.1",
        "240.0.0.1",
    ] {
        assert_eq!(classify(v4(address)), AddressClass::Special, "{address}");
    }
    assert_eq!(classify(v6("::")), AddressClass::Special);
    assert_eq!(classify(v6("2001:db8::1")), AddressClass::Special);
    assert_eq!(classify(v6("ff02::1")), AddressClass::Special);
}

#[test]
fn ordinary_public_addresses_are_global() {
    for address in [
        "1.1.1.1",
        "8.8.8.8",
        "93.184.216.34",
        "172.15.0.1",
        "172.32.0.1",
    ] {
        assert_eq!(classify(v4(address)), AddressClass::Global, "{address}");
    }
    assert_eq!(classify(v6("2606:4700::1111")), AddressClass::Global);
}

#[test]
fn an_ipv4_mapped_v6_is_classified_as_the_v4_it_is() {
    // The bypass this closes: ::ffff:127.0.0.1 is not "a global v6".
    assert_eq!(classify(v6("::ffff:127.0.0.1")), AddressClass::Loopback);
    assert_eq!(classify(v6("::ffff:10.0.0.1")), AddressClass::Private);
    assert_eq!(classify(v6("::ffff:1.1.1.1")), AddressClass::Global);
}

#[test]
fn bound_octets_are_exact() {
    // 172.15/16 and 172.32/16 are public; only 172.16/12 is private.
    assert!(is_global(v4("172.15.255.255")));
    assert!(!is_global(v4("172.16.0.0")));
    assert!(!is_global(v4("172.31.255.255")));
    assert!(is_global(v4("172.32.0.0")));
}

// ===== parsing =====

#[test]
fn a_plain_url_parses() {
    let target = parse_target("https://example.com/a/b?c=1#d").expect("parses");
    assert_eq!(target.scheme, "https");
    assert_eq!(target.host, "example.com");
    assert_eq!(target.port, 443);
}

#[test]
fn the_default_port_follows_the_scheme() {
    assert_eq!(
        parse_target("http://example.com/x").expect("parses").port,
        80
    );
    assert_eq!(
        parse_target("https://example.com/x").expect("parses").port,
        443
    );
    assert_eq!(
        parse_target("http://example.com:8080/x")
            .expect("parses")
            .port,
        8080
    );
}

#[test]
fn an_explicit_port_is_kept() {
    assert_eq!(
        parse_target("http://127.0.0.1:8080/x")
            .expect("parses")
            .port,
        8080
    );
    assert_eq!(
        parse_target("http://localhost:11434").expect("parses").port,
        11434
    );
}

#[test]
fn userinfo_is_not_the_host() {
    // http://user:pass@evil@good.com — the host is the part after the last @.
    let target = parse_target("https://user:pass@example.com/x").expect("parses");
    assert_eq!(target.host, "example.com");
}

#[test]
fn a_bracketed_ipv6_literal_parses() {
    let target = parse_target("http://[::1]:8080/x").expect("parses");
    assert_eq!(target.host, "::1");
    assert_eq!(target.port, 8080);
}

#[test]
fn the_scheme_is_case_insensitive_but_the_rule_is_not() {
    assert_eq!(
        parse_target("HTTPS://Example.com").expect("parses").scheme,
        "https"
    );
}

#[test]
fn every_non_http_scheme_is_refused() {
    for url in [
        "file:///etc/passwd",
        "ftp://example.com/x",
        "gopher://example.com/",
        "data:text/html,hi",
        "javascript:alert(1)",
        "no-scheme-at-all",
    ] {
        assert!(
            matches!(parse_target(url), Err(GuardError::Scheme(_))),
            "{url} must be refused"
        );
    }
}

#[test]
fn a_url_without_a_host_is_refused() {
    assert_eq!(parse_target("https:///path"), Err(GuardError::NoHost));
}

#[test]
fn the_refusal_reads_like_a_sentence() {
    assert_eq!(
        GuardError::Scheme("file".into()).to_string(),
        "only http and https URLs can be fetched, not file"
    );
    assert_eq!(
        GuardError::NotGlobal {
            host: "localhost".into(),
            address: v4("127.0.0.1"),
            class: AddressClass::Loopback,
        }
        .to_string(),
        "localhost resolves to 127.0.0.1, which is the loopback interface"
    );
}

// ===== Bing's redirect =====

#[test]
fn a_bing_redirect_decodes_to_its_target() {
    // The literal href measured on a live page (`docs/tools/web-search.md` §1),
    // base64url of https://rustify.rs/articles/rust-async-runtimes-tokio-vs-
    // async-std-2026.
    let href = "https://www.bing.com/ck/a?!&&p=a8db26a8b5696c652a71f2b0c2527a57355c8211c\
                &u=a1aHR0cHM6Ly9ydXN0aWZ5LnJzL2FydGljbGVzL3J1c3QtYXN5bmMtcnVudGltZXMtdG9raW8tdnMtYXN5bmMtc3RkLTIwMjY&ntb=1";
    assert_eq!(
        unwrap_bing_redirect(href),
        "https://rustify.rs/articles/rust-async-runtimes-tokio-vs-async-std-2026"
    );
}

#[test]
fn an_html_escaped_redirect_still_decodes() {
    let href = "https://www.bing.com/ck/a?u=a1aHR0cHM6Ly9leGFtcGxlLmNvbS9wYWdl&amp;ntb=1";
    assert_eq!(unwrap_bing_redirect(href), "https://example.com/page");
}

#[test]
fn an_ordinary_href_passes_through() {
    for href in ["https://example.com/x", "/relative/path"] {
        assert_eq!(unwrap_bing_redirect(href), href);
    }
    assert_eq!(
        unwrap_bing_redirect("https://example.com/x?a=1&amp;b=2"),
        "https://example.com/x?a=1&b=2",
        "entities are unescaped even when nothing decodes"
    );
}

#[test]
fn an_undecodable_redirect_is_left_as_it_was() {
    // Better a Bing URL than an empty or garbled one: the caller can still see
    // that the source returned something.
    for href in [
        "https://www.bing.com/ck/a?u=NOTBASE64!!!",
        "https://www.bing.com/ck/a?u=bm90Y2xpY2t1cmw",
        "https://www.bing.com/ck/a?u=b64",
    ] {
        assert_eq!(unwrap_bing_redirect(href), href, "{href}");
    }
}

#[test]
fn base64url_handles_every_length() {
    // Padding is optional in the URL form; all three cases must round-trip.
    for (encoded, plain) in [
        ("aHR0cHM6Ly9leGFtcGxlLmNvbS9h", "https://example.com/a"),
        ("aHR0cHM6Ly9leGFtcGxlLmNvbS9hYg", "https://example.com/ab"),
        ("aHR0cHM6Ly9leGFtcGxlLmNvbS9hYmM", "https://example.com/abc"),
    ] {
        let got = base64url_decode(encoded).expect("decodes");
        assert_eq!(String::from_utf8(got).unwrap(), plain);
    }
}
