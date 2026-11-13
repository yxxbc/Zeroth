//! The address gate for `web_fetch` (`docs/tools/web-search.md` *`web_fetch`*).
//!
//! This is the one tool that can be pointed at an arbitrary target, so the
//! target is classified **before** a connection exists: the scheme must be
//! http(s), the host is resolved, every resolved address must be globally
//! routable, and the caller pins the address that passed
//! (`ClientBuilder::resolve`) so a second DNS answer cannot swap it. A
//! redirect is re-gated at every hop.
//!
//! All of it is pure — an address string in, a class out — so the ranges are
//! testable without opening a socket. The resolution itself is the boundary's
//! (`crate::llm::web_exec`).

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// What an address is, for the gate's decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressClass {
    /// Globally routable: the only thing `web_fetch` may connect to.
    Global,
    /// The loopback interface.
    Loopback,
    /// A private network (`10/8`, `172.16/12`, `192.168/16`, `fc00::/7`).
    Private,
    /// Link-local, including the cloud metadata endpoint `169.254.169.254`.
    LinkLocal,
    /// Everything else that is not globally routable: `0.0.0.0`, multicast,
    /// documentation ranges, broadcast, `::`.
    Special,
}

/// Why a target was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardError {
    /// The scheme is not `http` or `https`.
    Scheme(String),
    /// No host in the URL.
    NoHost,
    /// The host resolved to nothing.
    Unresolved(String),
    /// A host that resolved, but to an address that is not globally routable.
    NotGlobal {
        /// The host as written.
        host: String,
        /// The address that failed.
        address: IpAddr,
        /// What that address is.
        class: AddressClass,
    },
    /// A redirect chain longer than the ceiling.
    TooManyRedirects(usize),
}

impl std::fmt::Display for GuardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GuardError::Scheme(scheme) => {
                write!(f, "only http and https URLs can be fetched, not {scheme}")
            }
            GuardError::NoHost => f.write_str("the URL has no host"),
            GuardError::Unresolved(host) => write!(f, "{host} did not resolve"),
            GuardError::NotGlobal {
                host,
                address,
                class,
            } => write!(
                f,
                "{host} resolves to {address}, which is {}",
                match class {
                    AddressClass::Global => "global",
                    AddressClass::Loopback => "the loopback interface",
                    AddressClass::Private => "a private address",
                    AddressClass::LinkLocal => "a link-local address",
                    AddressClass::Special => "not a routable address",
                }
            ),
            GuardError::TooManyRedirects(n) => {
                write!(f, "more than {n} redirects")
            }
        }
    }
}

/// A parsed target: the scheme, the host to resolve, and the port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// `http` or `https`, lowercased.
    pub scheme: String,
    /// The host as written (a name or a literal address).
    pub host: String,
    /// The effective port.
    pub port: u16,
}

/// Parse a URL the gate will accept, refusing a scheme before any resolution
/// happens.
///
/// # Errors
///
/// A non-http(s) scheme or a URL with no host ([`GuardError`]).
pub fn parse_target(url: &str) -> Result<Target, GuardError> {
    let trimmed = url.trim();
    let (scheme, rest) = trimmed
        .split_once("://")
        .ok_or_else(|| GuardError::Scheme(trimmed.to_string()))?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err(GuardError::Scheme(scheme));
    }
    let rest = rest.split(['?', '#']).next().unwrap_or("");
    let authority = rest.split('/').next().unwrap_or("");
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    if authority.is_empty() {
        return Err(GuardError::NoHost);
    }
    // The default port follows the scheme — `http://` is 80, not 443. A
    // request to port 443 with plain HTTP would fail in a way that reads like
    // a network problem, which is the worst kind of bug to debug.
    let default_port = if scheme == "https" { 443 } else { 80 };
    // A bare IPv6 literal is bracketed; `host:port` splits on the last colon
    // only when it is not inside brackets.
    let (host, port) = if let Some(stripped) = authority.strip_prefix('[') {
        let (inside, after) = stripped.split_once(']').unwrap_or((stripped, ""));
        let port = after
            .strip_prefix(':')
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(default_port);
        (inside.to_string(), port)
    } else if let Some((host, port)) = authority.rsplit_once(':') {
        if port.chars().all(|c| c.is_ascii_digit()) && !port.is_empty() {
            (
                host.to_string(),
                port.parse::<u16>().unwrap_or(default_port),
            )
        } else {
            (authority.to_string(), default_port)
        }
    } else {
        (authority.to_string(), default_port)
    };
    if host.is_empty() {
        return Err(GuardError::NoHost);
    }
    Ok(Target { scheme, host, port })
}

/// Whether an address is globally routable — the one class `web_fetch` may
/// connect to.
#[must_use]
pub fn is_global(address: IpAddr) -> bool {
    classify(address) == AddressClass::Global
}

/// Classify an address.
#[must_use]
pub fn classify(address: IpAddr) -> AddressClass {
    match address {
        IpAddr::V4(v4) => classify_v4(v4),
        IpAddr::V6(v6) => classify_v6(v6),
    }
}

fn classify_v4(address: Ipv4Addr) -> AddressClass {
    let octets = address.octets();
    let (a, b) = (octets[0], octets[1]);
    if address.is_loopback() {
        return AddressClass::Loopback;
    }
    if address.is_link_local() {
        // 169.254.0.0/16, including the metadata endpoint 169.254.169.254.
        return AddressClass::LinkLocal;
    }
    if address.is_private() {
        return AddressClass::Private;
    }
    if address.is_unspecified()
        || address.is_broadcast()
        || address.is_multicast()
        || address.is_documentation()
    {
        return AddressClass::Special;
    }
    // 100.64/10 carrier-grade NAT, and 192.0.0.0/24 / 198.18/15 / 240/4.
    if (a == 100 && (64..128).contains(&b))
        || (a == 192 && b == 0)
        || (a == 198 && (18..20).contains(&b))
        || a >= 240
    {
        return AddressClass::Special;
    }
    AddressClass::Global
}

fn classify_v6(address: Ipv6Addr) -> AddressClass {
    if address.is_loopback() {
        return AddressClass::Loopback;
    }
    if address.is_unspecified() {
        return AddressClass::Special;
    }
    let segments = address.segments();
    // fc00::/7 unique-local.
    if segments[0] & 0xfe00 == 0xfc00 {
        return AddressClass::Private;
    }
    // fe80::/10 link-local.
    if segments[0] & 0xffc0 == 0xfe80 {
        return AddressClass::LinkLocal;
    }
    if address.is_multicast() {
        return AddressClass::Special;
    }
    // An IPv4-mapped address (::ffff:a.b.c.d) is classified as the v4 it is,
    // so 127.0.0.1 cannot slip through as "global v6".
    if let Some(v4) = address.to_ipv4_mapped() {
        return classify_v4(v4);
    }
    // 2001:db8::/32 documentation, 2001::/32 Teredo.
    if segments[0] == 0x2001 && (segments[1] == 0x0db8 || segments[1] == 0) {
        return AddressClass::Special;
    }
    AddressClass::Global
}

/// Decode a Bing redirect: `/ck/a?u=a1<base64url>` → the real URL. Bing's
/// result markup has no plain href, and the encoding is stable — verified
/// 5 of 5 on a live page (`docs/tools/web-search.md` *Measured* §1).
///
/// A URL that is not a Bing redirect is returned unchanged, so the caller can
/// pass every href through this.
#[must_use]
pub fn unwrap_bing_redirect(href: &str) -> String {
    let decoded = html_unescape(href);
    let Some(query) = decoded.split_once('?').map(|(_, q)| q) else {
        return decoded;
    };
    let Some(u) = query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == "u").then_some(v)
    }) else {
        return decoded;
    };
    let Some(encoded) = u.strip_prefix("a1") else {
        return decoded;
    };
    match base64url_decode(encoded) {
        Some(bytes) => match String::from_utf8(bytes) {
            Ok(url) if url.starts_with("http") => url,
            _ => decoded,
        },
        None => decoded,
    }
}

/// `&amp;` → `&`, and the few other entities an href carries.
fn html_unescape(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

/// base64url, padding optional.
fn base64url_decode(text: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let mut buffer: u32 = 0;
    let mut bits = 0u32;
    for byte in text.bytes() {
        if byte == b'=' {
            break;
        }
        let value = TABLE.iter().position(|c| *c == byte)? as u32;
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests;
