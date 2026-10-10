//! Fetching a web link to a `.torrent` file, carefully.
//!
//! The engine can fetch web links itself, but without any limits (any size, any number of
//! redirects, any address). So BitDinghy fetches them here instead, and only then hands the file
//! to `torrent_file.rs`, which checks and rebuilds it like any other `.torrent`:
//!
//! * **https only**, unless the person allowed plain http in Settings. A plain http link can be
//!   changed on the way, so a different torrent could be swapped in without anyone noticing.
//! * **Ordinary internet addresses only**, the same rule as for trackers: never this computer or
//!   the local network (a link must not make BitDinghy poke at a router or a printer). This is
//!   checked for the link AND for every place it redirects to.
//! * **At most [`MAX_REDIRECTS`] redirects, [`TIME_LIMIT`] in all, and at most
//!   [`MAX_TORRENT_BYTES`]** (a `.torrent` is kilobytes).
//!
//! Like the tracker filter, this looks at how an address is WRITTEN; a normal-looking name that
//! has been pointed at a local address isn't caught (see `TODO.md`).

use std::time::Duration;

use url::Url;

use crate::error::{Error, Result};
use crate::magnet;
use crate::torrent_file::{self, CheckedTorrent, MAX_TORRENT_BYTES};

/// How many times a link may send BitDinghy on to another address.
pub const MAX_REDIRECTS: usize = 5;

/// How long fetching one `.torrent` file may take in all.
pub const TIME_LIMIT: Duration = Duration::from_secs(30);

/// A web link longer than this is certainly not a normal one.
const MAX_LINK_BYTES: usize = 4096;

/// Which links may be fetched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rules {
    /// Also plain `http://` links (the person's choice in Settings).
    pub allow_http: bool,
    /// Addresses on this computer or the local network. Only the tests here turn this on (their
    /// pretend web server runs on this computer).
    allow_local: bool,
}

impl Rules {
    pub fn new(allow_http: bool) -> Rules {
        Rules {
            allow_http,
            allow_local: false,
        }
    }
}

fn problem(message: impl Into<String>) -> Error {
    Error::WebLink(message.into())
}

/// Checks a web link before anything is fetched.
pub fn check_link(text: &str, rules: Rules) -> Result<Url> {
    let text = text.trim();
    if text.len() > MAX_LINK_BYTES {
        return Err(problem("That web link is far too long."));
    }
    let url = Url::parse(text).map_err(|_| problem("That isn't a valid web link."))?;
    check_address(&url, rules).map_err(problem)?;
    Ok(url)
}

/// The rules for one address (the link itself, or where it redirects to).
fn check_address(url: &Url, rules: Rules) -> std::result::Result<(), String> {
    match url.scheme() {
        "https" => {}
        "http" if rules.allow_http => {}
        "http" => {
            return Err(
                "BitDinghy only opens secure (https://) links to .torrent files, because a plain \
                 http:// link can be changed on the way. You can allow plain http links in \
                 Settings."
                    .to_string(),
            );
        }
        _ => {
            return Err("BitDinghy only opens web links (https://) to .torrent files.".to_string());
        }
    }
    let host = url
        .host()
        .ok_or_else(|| "That web link has no address in it.".to_string())?;
    if !rules.allow_local && !magnet::is_internet_host(&host) {
        return Err(format!(
            "BitDinghy doesn't open links to this computer or the local network ({host})."
        ));
    }
    Ok(())
}

/// Fetches the `.torrent` file a web link points to, and checks and rebuilds it.
pub async fn fetch(text: &str, rules: Rules) -> Result<CheckedTorrent> {
    let url = check_link(text, rules)?;
    fetch_checked(url, rules).await
}

/// The fetching itself, for a link that has passed [`check_link`].
async fn fetch_checked(url: Url, rules: Rules) -> Result<CheckedTorrent> {
    let host = url.host_str().unwrap_or_default().to_string();
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= MAX_REDIRECTS {
                attempt.error("the link sent BitDinghy on too many times")
            } else if let Err(reason) = check_address(attempt.url(), rules) {
                attempt.error(reason)
            } else {
                attempt.follow()
            }
        }))
        .timeout(TIME_LIMIT)
        .connect_timeout(Duration::from_secs(10))
        .user_agent(format!("{}/{}", crate::APP_NAME, crate::VERSION))
        .build()
        .map_err(|e| {
            problem(format!(
                "BitDinghy couldn't get ready to fetch the link ({e})."
            ))
        })?;

    let failed = |error: reqwest::Error| {
        problem(format!(
            "BitDinghy couldn't fetch the .torrent file from {host}: {}.",
            describe(&error)
        ))
    };
    let too_big = || {
        problem(format!(
            "{host} sent something far too big to be a .torrent file."
        ))
    };

    let mut response = client.get(url.clone()).send().await.map_err(failed)?;
    let status = response.status();
    if !status.is_success() {
        return Err(problem(format!(
            "{host} didn't send the .torrent file (it answered \"{status}\")."
        )));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_TORRENT_BYTES)
    {
        return Err(too_big());
    }
    let mut bytes = Vec::new();
    while let Some(piece) = response.chunk().await.map_err(failed)? {
        bytes.extend_from_slice(&piece);
        if bytes.len() as u64 > MAX_TORRENT_BYTES {
            return Err(too_big());
        }
    }
    torrent_file::check(&bytes).map_err(|_| {
        problem(format!(
            "That link didn't lead to a .torrent file ({host} sent something else)."
        ))
    })
}

/// A fetching problem in plain words, with its underlying reason.
fn describe(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        return format!("it took longer than {} seconds", TIME_LIMIT.as_secs());
    }
    // reqwest's own words, then the deeper reasons (for example our redirect rule).
    let mut words = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(reason) = source {
        words.push_str(": ");
        words.push_str(&reason.to_string());
        source = reason.source();
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// The info part of a tiny one-file torrent.
    const TORRENT: &str = "d4:infod6:lengthi5e4:name9:hello.txt12:piece lengthi16384e6:pieces20:AAAAAAAAAAAAAAAAAAAAee";

    /// Rules for the pretend web server below, which runs on this computer over plain http.
    fn test_rules(allow_http: bool) -> Rules {
        Rules {
            allow_http,
            allow_local: true,
        }
    }

    /// A pretend web server on this computer. It answers each of `answers` once, in order, one
    /// connection each; `{base}` in an answer becomes the server's own address.
    async fn web_server(answers: Vec<String>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let base_for_answers = base.clone();
        tokio::spawn(async move {
            for answer in answers {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                let mut request = vec![0u8; 4096];
                let _ = stream.read(&mut request).await;
                let answer = answer.replace("{base}", &base_for_answers);
                let _ = stream.write_all(answer.as_bytes()).await;
                let _ = stream.shutdown().await;
            }
        });
        base
    }

    fn ok(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    fn redirect(to: &str) -> String {
        format!(
            "HTTP/1.1 302 Found\r\nLocation: {to}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
    }

    #[test]
    fn https_links_are_fine_and_http_needs_the_setting() {
        assert!(
            check_link(
                "https://archlinux.org/releng/releases/x/torrent/",
                Rules::new(false)
            )
            .is_ok()
        );
        let error = check_link("http://example.org/x.torrent", Rules::new(false))
            .unwrap_err()
            .to_string();
        assert!(error.contains("Settings"), "{error}");
        assert!(check_link("http://example.org/x.torrent", Rules::new(true)).is_ok());
    }

    #[test]
    fn links_to_this_computer_or_the_local_network_are_refused() {
        for link in [
            "https://localhost/x.torrent",
            "https://127.0.0.1/x.torrent",
            "https://192.168.1.1/x.torrent",
            "https://10.0.0.5:8080/x.torrent",
            "https://[::1]/x.torrent",
            "https://printer.local/x.torrent",
        ] {
            let error = check_link(link, Rules::new(true)).unwrap_err().to_string();
            assert!(error.contains("local network"), "{link}: {error}");
        }
    }

    #[test]
    fn other_kinds_of_link_are_refused() {
        for link in [
            "ftp://example.org/x.torrent",
            "file:///C:/x.torrent",
            "javascript:alert(1)",
            "not a link",
        ] {
            assert!(check_link(link, Rules::new(true)).is_err(), "{link}");
        }
        assert!(
            check_link(
                &format!("https://example.org/{}", "a".repeat(5000)),
                Rules::new(true)
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn a_torrent_is_fetched_checked_and_rebuilt() {
        let base = web_server(vec![ok(TORRENT)]).await;
        let url = Url::parse(&format!("{base}/x.torrent")).unwrap();
        let torrent = fetch_checked(url, test_rules(true)).await.unwrap();
        assert_eq!(torrent.display_name.as_deref(), Some("hello.txt"));
    }

    #[tokio::test]
    async fn a_few_redirects_are_followed() {
        let base = web_server(vec![
            redirect("{base}/second"),
            redirect("{base}/third"),
            ok(TORRENT),
        ])
        .await;
        let url = Url::parse(&format!("{base}/first")).unwrap();
        assert!(fetch_checked(url, test_rules(true)).await.is_ok());
    }

    #[tokio::test]
    async fn too_many_redirects_are_refused() {
        let base = web_server(vec![redirect("{base}/again"); MAX_REDIRECTS + 2]).await;
        let url = Url::parse(&format!("{base}/first")).unwrap();
        let error = fetch_checked(url, test_rules(true))
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("too many times"), "{error}");
    }

    #[tokio::test]
    async fn a_redirect_to_plain_http_is_refused_without_the_setting() {
        // As if a secure site sent BitDinghy on to a plain http address.
        let base = web_server(vec![redirect("{base}/plain"), ok(TORRENT)]).await;
        let url = Url::parse(&format!("{base}/start")).unwrap();
        let error = fetch_checked(url, test_rules(false))
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("https"), "{error}");
    }

    #[tokio::test]
    async fn a_redirect_into_the_local_network_is_refused() {
        let base = web_server(vec![redirect("http://192.168.1.1/admin")]).await;
        let url = Url::parse(&format!("{base}/start")).unwrap();
        let rules = Rules {
            allow_http: true,
            allow_local: false,
        };
        // (The pretend server itself is on this computer, so start past the first check.)
        let error = fetch_checked(url, rules).await.unwrap_err().to_string();
        assert!(error.contains("local network"), "{error}");
    }

    #[tokio::test]
    async fn web_server_problems_are_explained() {
        let base = web_server(vec![
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
            ok("<html>a web page, not a torrent</html>"),
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                MAX_TORRENT_BYTES + 1
            ),
        ])
        .await;
        let get = |path: &str| Url::parse(&format!("{base}/{path}")).unwrap();
        let error = fetch_checked(get("a"), test_rules(true))
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("404"), "{error}");
        let error = fetch_checked(get("b"), test_rules(true))
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("didn't lead to a .torrent file"), "{error}");
        let error = fetch_checked(get("c"), test_rules(true))
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("far too big"), "{error}");
    }
}
