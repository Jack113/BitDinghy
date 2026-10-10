//! Reading magnet links safely.
//!
//! A magnet link usually comes from a web page or a message, so it is untrusted text. The
//! BitTorrent engine's own parser is generous in what it accepts (for example a `so=` file-range
//! with no upper limit, which a hostile link can use to make it try to allocate an enormous
//! list). So we never hand the raw text to the engine. Instead we read out only the parts we
//! understand and validate, and build a fresh minimal link from them:
//!
//! * `xt`: the torrent's fingerprint (the "info hash"), strictly validated
//! * `dn`: a display name, stripped of control and text-direction characters and shortened
//! * `tr`: a few tracker addresses, only ordinary internet web/UDP addresses
//!
//! Everything else in the link is ignored on purpose.

use std::net::IpAddr;

use url::{Host, Url};

use crate::error::{Error, Result};

const MAX_LINK_BYTES: usize = 16 * 1024;
const MAX_QUERY_PAIRS: usize = 200;
pub(crate) const MAX_TRACKERS: usize = 20;
const MAX_TRACKER_BYTES: usize = 512;
const MAX_NAME_CHARS: usize = 120;

/// A magnet link that has been checked and rebuilt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedMagnet {
    /// The torrent's fingerprint as 40 hexadecimal characters.
    pub info_hash: String,
    /// A cleaned-up name taken from the link, if it had one.
    pub display_name: Option<String>,
    /// A rebuilt, minimal magnet link. This is the only text ever given to the engine.
    pub clean_link: String,
}

fn invalid(message: &str) -> Error {
    Error::InvalidMagnet(message.to_string())
}

fn not_a_magnet_link() -> Error {
    invalid(
        "That isn't a magnet link. Magnet links start with magnet:?xt=urn:btih: \
         (links to .torrent files aren't supported yet).",
    )
}

/// Text-direction control characters can make a name display differently from what it really
/// is (for example, making `evilexe.txt` look like `eviltxt.exe`). We drop them.
fn is_direction_control(c: char) -> bool {
    matches!(c, '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

/// Makes a name from the internet safe to SHOW: no control or text-direction characters, one
/// space between words, and a limited length. This is for display only. File names on disk go
/// through the guard in `safety`.
pub fn clean_display_name(name: &str) -> Option<String> {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .filter(|c| !c.is_control() && !is_direction_control(*c))
        .collect();
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    Some(collapsed.chars().take(MAX_NAME_CHARS).collect())
}

/// Accepts only ordinary internet trackers. Trackers written as an address that points back at
/// this computer or at the local network (or with an obviously local name such as `localhost` or
/// `printer.local`) are dropped.
///
/// This is a first line of defence, not a complete one: it looks at how the tracker is WRITTEN. A
/// normal-looking host name that an attacker has pointed at a local address would still get
/// through, and closing that needs control over the engine's own connections (see `TODO.md`).
pub(crate) fn clean_tracker(value: &str) -> Option<String> {
    if value.len() > MAX_TRACKER_BYTES {
        return None;
    }
    let url = Url::parse(value).ok()?;
    if !matches!(url.scheme(), "http" | "https" | "udp") {
        return None;
    }
    let keep = match url.host()? {
        Host::Domain(domain) => {
            let domain = domain.to_ascii_lowercase();
            !(domain == "localhost" || domain.ends_with(".localhost") || domain.ends_with(".local"))
        }
        Host::Ipv4(ip) => is_public(IpAddr::V4(ip)),
        Host::Ipv6(ip) => is_public(IpAddr::V6(ip)),
    };
    keep.then(|| url.to_string())
}

fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            !(ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || ip.is_multicast())
        }
        IpAddr::V6(ip) => {
            !(ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || ip.is_unique_local()
                || ip.is_unicast_link_local())
        }
    }
}

/// A fingerprint is 40 hexadecimal characters, or 32 base32 characters (an older style).
fn looks_like_info_hash(candidate: &str) -> bool {
    match candidate.len() {
        40 => candidate.chars().all(|c| c.is_ascii_hexdigit()),
        32 => candidate
            .chars()
            .all(|c| c.is_ascii_alphabetic() || ('2'..='7').contains(&c)),
        _ => false,
    }
}

/// Checks a pasted magnet link and rebuilds it in a minimal, safe form.
pub fn parse(input: &str) -> Result<ParsedMagnet> {
    let text = input.trim();
    if text.is_empty() {
        return Err(invalid("Paste a magnet link first."));
    }
    if text.len() > MAX_LINK_BYTES {
        return Err(invalid("That text is far too long to be a magnet link."));
    }
    if text.chars().any(|c| c.is_control()) {
        return Err(invalid(
            "That doesn't look like a single magnet link (it has line breaks or hidden characters).",
        ));
    }

    let url = Url::parse(text).map_err(|_| not_a_magnet_link())?;
    if url.scheme() != "magnet" {
        return Err(not_a_magnet_link());
    }

    let mut btih: Option<String> = None;
    let mut saw_v2_only_hash = false;
    let mut display_name: Option<String> = None;
    let mut trackers: Vec<String> = Vec::new();

    for (key, value) in url.query_pairs().take(MAX_QUERY_PAIRS) {
        match key.as_ref() {
            "xt" => {
                if let Some(hash) = value.strip_prefix("urn:btih:") {
                    btih.get_or_insert_with(|| hash.to_string());
                } else if value.starts_with("urn:btmh:") {
                    saw_v2_only_hash = true;
                }
            }
            "dn" => {
                if display_name.is_none() {
                    display_name = clean_display_name(&value);
                }
            }
            "tr" => {
                if trackers.len() < MAX_TRACKERS
                    && let Some(tracker) = clean_tracker(&value)
                    && !trackers.contains(&tracker)
                {
                    trackers.push(tracker);
                }
            }
            // Everything else (so=, x.pe=, xs=, ws=, ...) is deliberately ignored.
            _ => {}
        }
    }

    let Some(candidate) = btih else {
        if saw_v2_only_hash {
            return Err(invalid(
                "This magnet link only has the newer BitTorrent v2 fingerprint, which BitDinghy \
                 can't use yet.",
            ));
        }
        return Err(invalid(
            "This magnet link has no torrent fingerprint (it should contain xt=urn:btih:...). \
             Try copying the whole link again.",
        ));
    };
    let bad_hash = || {
        invalid(
            "The torrent fingerprint in this magnet link isn't valid. \
             Try copying the whole link again.",
        )
    };
    if !looks_like_info_hash(&candidate) {
        return Err(bad_hash());
    }
    // The engine's own reader turns either spelling of the fingerprint into the standard form.
    let id20 = librqbit::Magnet::parse(&format!("magnet:?xt=urn:btih:{candidate}"))
        .ok()
        .and_then(|magnet| magnet.as_id20())
        .ok_or_else(bad_hash)?;
    let info_hash = id20.as_string();
    let clean_link = build_link(&info_hash, display_name.as_deref(), &trackers);
    Ok(ParsedMagnet {
        info_hash,
        display_name,
        clean_link,
    })
}

/// Builds a minimal magnet link from parts that have ALREADY been checked: a 40-character
/// fingerprint, a cleaned name and cleaned trackers. (`.torrent` files use this too, so every
/// download in the saved list has a clean link to fall back on.)
pub(crate) fn build_link(info_hash: &str, name: Option<&str>, trackers: &[String]) -> String {
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    query.append_pair("xt", &format!("urn:btih:{info_hash}"));
    if let Some(name) = name {
        query.append_pair("dn", name);
    }
    for tracker in trackers {
        query.append_pair("tr", tracker);
    }
    format!("magnet:?{}", query.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "c9e15763f722f23e98a29decdfae341b98d53056";

    fn message(result: Result<ParsedMagnet>) -> String {
        result.unwrap_err().to_string()
    }

    #[test]
    fn a_plain_magnet_link_is_accepted() {
        let parsed = parse(&format!("magnet:?xt=urn:btih:{HASH}")).unwrap();
        assert_eq!(parsed.info_hash, HASH);
        assert_eq!(parsed.display_name, None);
        assert_eq!(parsed.clean_link, format!("magnet:?xt=urn%3Abtih%3A{HASH}"));
    }

    #[test]
    fn surrounding_spaces_and_upper_case_hashes_are_fine() {
        let parsed = parse(&format!("  magnet:?xt=urn:btih:{}  ", HASH.to_uppercase())).unwrap();
        assert_eq!(parsed.info_hash, HASH);
    }

    #[test]
    fn the_older_base32_fingerprint_is_converted() {
        // 32 base32 characters encode the same 20 bytes as 40 hex characters.
        let parsed = parse("magnet:?xt=urn:btih:ZHQVOY7XELZD5GFCTXG56XRUDOMNKMCW").unwrap();
        assert_eq!(parsed.info_hash.len(), 40);
        assert!(parsed.info_hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn name_and_public_trackers_are_kept() {
        let link = format!(
            "magnet:?xt=urn:btih:{HASH}&dn=Ubuntu%2024.04&tr=udp%3A%2F%2Ftracker.example.org%3A6969\
             &tr=https%3A%2F%2Ftracker.example.net%2Fannounce"
        );
        let parsed = parse(&link).unwrap();
        assert_eq!(parsed.display_name.as_deref(), Some("Ubuntu 24.04"));
        assert!(parsed.clean_link.contains("tracker.example.org"));
        assert!(parsed.clean_link.contains("tracker.example.net"));
        // And the rebuilt link reads back to the same thing.
        assert_eq!(parse(&parsed.clean_link).unwrap(), parsed);
    }

    #[test]
    fn a_hostile_file_range_never_reaches_the_engine() {
        // The engine's own parser would try to list 18 quintillion file numbers for this.
        let link = format!("magnet:?xt=urn:btih:{HASH}&so=0-18446744073709551615");
        let parsed = parse(&link).unwrap();
        assert!(!parsed.clean_link.contains("so="));
        assert!(!parsed.clean_link.contains("18446744073709551615"));
    }

    #[test]
    fn unknown_parameters_are_dropped() {
        let link =
            format!("magnet:?xt=urn:btih:{HASH}&xs=http%3A%2F%2Fevil.example&x.pe=1.2.3.4%3A5");
        let parsed = parse(&link).unwrap();
        assert!(!parsed.clean_link.contains("evil"));
        assert!(!parsed.clean_link.contains("x.pe"));
    }

    #[test]
    fn trackers_that_point_at_this_pc_or_the_local_network_are_dropped() {
        for tracker in [
            "http://localhost/announce",
            "http://127.0.0.1:8080/announce",
            "http://192.168.1.1/admin",
            "http://10.0.0.5/",
            "http://172.16.0.9/",
            "http://169.254.1.1/",
            "http://[::1]/",
            "http://[fd00::1]/",
            "http://printer.local/",
            "ftp://tracker.example.org/",
            "file:///etc/passwd",
        ] {
            let link = format!(
                "magnet:?xt=urn:btih:{HASH}&tr={}",
                url::form_urlencoded::byte_serialize(tracker.as_bytes()).collect::<String>()
            );
            let parsed = parse(&link).unwrap();
            assert!(!parsed.clean_link.contains("tr="), "{tracker} was kept");
        }
    }

    #[test]
    fn too_many_trackers_are_limited() {
        let mut link = format!("magnet:?xt=urn:btih:{HASH}");
        for n in 0..50 {
            link.push_str(&format!("&tr=udp%3A%2F%2Ftracker{n}.example.org%3A1"));
        }
        let parsed = parse(&link).unwrap();
        assert_eq!(parsed.clean_link.matches("tr=").count(), MAX_TRACKERS);
    }

    #[test]
    fn names_are_cleaned_for_display() {
        let link =
            format!("magnet:?xt=urn:btih:{HASH}&dn=evil%0A%1B%5B31m%20%E2%80%AEgpj.exe%20%20%20x");
        let name = parse(&link).unwrap().display_name.unwrap();
        assert!(!name.chars().any(|c| c.is_control()));
        assert!(!name.contains('\u{202e}'));
        assert!(!name.contains("  "));
        let long = format!("magnet:?xt=urn:btih:{HASH}&dn={}", "a".repeat(5000));
        assert_eq!(
            parse(&long).unwrap().display_name.unwrap().chars().count(),
            MAX_NAME_CHARS
        );
    }

    #[test]
    fn non_magnet_input_is_explained_not_guessed() {
        assert!(message(parse("")).contains("Paste a magnet link"));
        assert!(message(parse("hello there")).contains("isn't a magnet link"));
        assert!(message(parse("https://example.org/file.torrent")).contains(".torrent"));
        assert!(message(parse(HASH)).contains("isn't a magnet link"));
    }

    #[test]
    fn broken_fingerprints_are_refused() {
        assert!(message(parse("magnet:?xt=urn:btih:nothex")).contains("isn't valid"));
        assert!(
            message(parse(
                "magnet:?xt=urn:btih:zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"
            ))
            .contains("isn't valid")
        );
        assert!(message(parse("magnet:?dn=just-a-name")).contains("no torrent fingerprint"));
    }

    #[test]
    fn version_two_only_links_are_explained() {
        let link = format!("magnet:?xt=urn:btmh:1220{}", "ab".repeat(32));
        assert!(message(parse(&link)).contains("v2"));
    }

    #[test]
    fn a_hybrid_link_uses_its_version_one_fingerprint() {
        let link = format!(
            "magnet:?xt=urn:btmh:1220{}&xt=urn:btih:{HASH}",
            "ab".repeat(32)
        );
        assert_eq!(parse(&link).unwrap().info_hash, HASH);
    }

    #[test]
    fn oversized_or_multiline_input_is_refused() {
        assert!(message(parse(&"a".repeat(20_000))).contains("far too long"));
        let two_lines = format!("magnet:?xt=urn:btih:{HASH}\nmagnet:?xt=urn:btih:{HASH}");
        assert!(message(parse(&two_lines)).contains("single magnet link"));
    }

    #[test]
    fn a_doubled_prefix_is_refused_not_repaired() {
        // A copy-paste slip seen in a real link. The owner's decision: refuse it, don't guess.
        let link = format!("magnet:?magnet:?xt=urn:btih:{HASH}&dn=Something");
        assert!(message(parse(&link)).contains("no torrent fingerprint"));
    }

    #[test]
    fn only_the_fingerprint_name_and_trackers_survive() {
        let link = format!(
            "magnet:?xt=urn:btih:{HASH}&dn=Some%20Name&tr=udp%3A%2F%2Ftracker.example.org%3A1337\
             &tr=udp%3A%2F%2Fother.example.net%3A6969%2Fannounce&xl=123&ws=http%3A%2F%2Fweb.example\
             &kt=words&x.pe=1.2.3.4%3A5&as=http%3A%2F%2Fsource.example"
        );
        let clean = parse(&link).unwrap().clean_link;
        let keys: Vec<String> = Url::parse(&clean)
            .unwrap()
            .query_pairs()
            .map(|(key, _)| key.into_owned())
            .collect();
        assert_eq!(keys, ["xt", "dn", "tr", "tr"]);
    }
}
