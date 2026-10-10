//! The channel between the command line and the open window ("remote control").
//!
//! While the window is open it owns the only engine (see `saved_list.rs`: one BitDinghy at a time).
//! So that `dinghy` can still be used then, the window listens on a **local channel** and
//! `dinghy` sends it requests: "show me the list", "add this link", and so on.
//!
//! * **Never reachable from another computer.** On Windows the channel is a *named pipe* that
//!   refuses every connection coming over the network (`reject_remote_clients`). On Linux it is a
//!   socket file in the data folder that only its owner can open. It is never a network port and
//!   never a local web server, so web pages and other computers can't reach it.
//! * **One channel per data folder.** The pipe's name comes from the data folder's path (and the
//!   user's name), so a second portable copy of BitDinghy has its own channel.
//! * **Simple messages.** Each request and each reply is one line of JSON text. Every request says
//!   which version of these messages it speaks; a mismatch gets a plain answer, never a guess.
//!   A message longer than [`MAX_MESSAGE_BYTES`] ends the conversation.
//!
//! Everything a request asks for goes through the same engine calls as the window's own buttons,
//! with the same checks, so the channel adds no new way around them.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt,
};

use crate::engine::{DownloadSnapshot, Engine};
use crate::error::{Error, Result};

/// The version of the messages below. Change it whenever a message changes shape.
pub const PROTOCOL_VERSION: u32 = 1;

/// The longest message accepted, in bytes. A list of hundreds of downloads fits easily.
pub const MAX_MESSAGE_BYTES: usize = 4 * 1024 * 1024;

/// A request from the command line, with the message version it speaks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub version: u32,
    pub command: Command,
}

/// What the command line can ask the window to do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    /// Every download, as the window shows it right now.
    List,
    /// Add a magnet link, exactly as if it was pasted into the window. `peers` are extra
    /// addresses to try (`dinghy get --peer`).
    Add {
        link: String,
        peers: Vec<SocketAddr>,
    },
}

/// The window's answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Reply {
    /// The downloads, in the order the window shows them, and the folder new downloads go to.
    Downloads {
        downloads: Vec<DownloadSnapshot>,
        download_dir: PathBuf,
    },
    /// The link was added (or was already there). `folder` is the download folder it belongs to.
    Added { info_hash: String, folder: PathBuf },
    /// The request didn't work; the message is for the person.
    Problem { message: String },
}

/// Where the channel is: a pipe name on Windows, a socket file on Linux.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address(PathBuf);

impl Address {
    /// The channel for the BitDinghy that uses this data folder.
    pub fn for_data_dir(data_dir: &Path) -> Address {
        if cfg!(windows) {
            // Windows paths ignore upper/lower case, so the name must too.
            let folder = data_dir.to_string_lossy().to_lowercase();
            let user = std::env::var("USERNAME").unwrap_or_default();
            let id = fnv1a(format!("{folder}\u{0}{user}").as_bytes());
            Address(PathBuf::from(format!(r"\\.\pipe\BitDinghy-{id:016x}")))
        } else {
            Address(data_dir.join("bitdinghy.sock"))
        }
    }

    /// The pipe name or socket path.
    pub fn path(&self) -> &Path {
        &self.0
    }
}

/// A small, stable hash (FNV-1a), so the window and `dinghy` always compute the same name.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

fn channel_error(what: &str, error: impl std::fmt::Display) -> Error {
    Error::Remote(format!(
        "Couldn't talk to BitDinghy's open window ({what}: {error})."
    ))
}

// ---------------------------------------------------------------------------------------------
// Reading and writing one message.
// ---------------------------------------------------------------------------------------------

/// Reads one line. `Ok(None)` means the other side hung up cleanly.
async fn read_line<R: AsyncBufRead + Unpin>(reader: &mut R) -> std::io::Result<Option<String>> {
    let mut bytes = Vec::new();
    let read = (&mut *reader)
        .take(MAX_MESSAGE_BYTES as u64 + 1)
        .read_until(b'\n', &mut bytes)
        .await?;
    if read == 0 {
        return Ok(None);
    }
    if bytes.last() != Some(&b'\n') {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "a message was too long or cut off",
        ));
    }
    bytes.pop();
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "not text"))
}

async fn write_line<W: AsyncWrite + Unpin, T: Serialize>(
    writer: &mut W,
    message: &T,
) -> std::io::Result<()> {
    let mut text = serde_json::to_string(message)?;
    text.push('\n');
    writer.write_all(text.as_bytes()).await?;
    writer.flush().await
}

// ---------------------------------------------------------------------------------------------
// The window's side.
// ---------------------------------------------------------------------------------------------

/// Starts answering requests on `address`, in the background, for as long as the program runs.
/// Must be called from inside a tokio runtime. Fails if another program already uses this channel
/// (for example a second window on the same data folder).
pub fn start_server(engine: Arc<Engine>, address: &Address) -> Result<()> {
    platform::start_server(engine, address)
}

/// Answers one connection until the other side hangs up. Every request gets exactly one reply.
pub(crate) async fn serve_connection<S: AsyncRead + AsyncWrite>(
    engine: &Engine,
    stream: S,
) -> std::io::Result<()> {
    let (reader, mut writer) = tokio::io::split(stream);
    let mut reader = tokio::io::BufReader::new(reader);
    while let Some(line) = read_line(&mut reader).await? {
        let reply = match parse_request(&line) {
            Ok(command) => answer(engine, command).await,
            Err(message) => Reply::Problem { message },
        };
        write_line(&mut writer, &reply).await?;
    }
    Ok(())
}

/// Checks the version FIRST, so a newer `dinghy` with new kinds of request gets a clear answer
/// instead of "not understood".
fn parse_request(line: &str) -> std::result::Result<Command, String> {
    let not_understood = || "BitDinghy's window didn't understand that request.".to_string();
    let value: serde_json::Value = serde_json::from_str(line).map_err(|_| not_understood())?;
    let version = value.get("version").and_then(serde_json::Value::as_u64);
    if version != Some(u64::from(PROTOCOL_VERSION)) {
        return Err(format!(
            "This dinghy and the open BitDinghy window are different versions (they speak \
             versions {} and {PROTOCOL_VERSION}). Use the dinghy.exe that sits next to \
             BitDinghy.exe.",
            version.map_or_else(|| "unknown".to_string(), |v| v.to_string())
        ));
    }
    serde_json::from_value::<Request>(value)
        .map(|request| request.command)
        .map_err(|_| not_understood())
}

async fn answer(engine: &Engine, command: Command) -> Reply {
    match command {
        Command::List => Reply::Downloads {
            downloads: engine.snapshot(),
            download_dir: engine.download_dir().to_path_buf(),
        },
        Command::Add { link, peers } => match engine.add_magnet(&link, peers) {
            Ok(id) => Reply::Added {
                info_hash: engine
                    .snapshot()
                    .into_iter()
                    .find(|download| download.id == id)
                    .map(|download| download.info_hash)
                    .unwrap_or_default(),
                folder: engine
                    .download_dir_of(id)
                    .unwrap_or_else(|| engine.download_dir().to_path_buf()),
            },
            Err(error) => Reply::Problem {
                message: error.to_string(),
            },
        },
    }
}

// ---------------------------------------------------------------------------------------------
// The command line's side.
// ---------------------------------------------------------------------------------------------

/// A conversation with the open window.
pub struct Client {
    reader: tokio::io::BufReader<tokio::io::ReadHalf<platform::Stream>>,
    writer: tokio::io::WriteHalf<platform::Stream>,
}

impl Client {
    /// Connects to the window that uses this channel. `Ok(None)` means no window is listening
    /// (it is closed), which is normal: the command line then works on its own.
    pub async fn connect(address: &Address) -> Result<Option<Client>> {
        let Some(stream) = platform::connect(address).await? else {
            return Ok(None);
        };
        let (reader, writer) = tokio::io::split(stream);
        Ok(Some(Client {
            reader: tokio::io::BufReader::new(reader),
            writer,
        }))
    }

    /// Sends one request and waits for the reply. A `Reply::Problem` becomes an error.
    pub async fn ask(&mut self, command: Command) -> Result<Reply> {
        let request = Request {
            version: PROTOCOL_VERSION,
            command,
        };
        write_line(&mut self.writer, &request)
            .await
            .map_err(|e| channel_error("sending", e))?;
        let line = read_line(&mut self.reader)
            .await
            .map_err(|e| channel_error("reading the answer", e))?
            .ok_or_else(|| {
                Error::Remote("BitDinghy's window closed while answering.".to_string())
            })?;
        match serde_json::from_str::<Reply>(&line) {
            Ok(Reply::Problem { message }) => Err(Error::Remote(message)),
            Ok(reply) => Ok(reply),
            Err(e) => Err(channel_error("the answer wasn't understood", e)),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The two kinds of channel.
// ---------------------------------------------------------------------------------------------

#[cfg(windows)]
mod platform {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use tokio::net::windows::named_pipe::{
        ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
    };

    use super::{Address, channel_error, serve_connection};
    use crate::engine::Engine;
    use crate::error::{Error, Result};

    pub type Stream = NamedPipeClient;

    /// Windows' "all instances of the pipe are busy" error.
    const ERROR_PIPE_BUSY: i32 = 231;

    /// One waiting end of the pipe. `reject_remote_clients` is what keeps every other computer
    /// out. (It is tokio's default too; it is set here so the rule is visible.) Windows' standard
    /// protection for a new pipe lets only this user (and the system and administrators) send to
    /// it; other users can at most open it for reading, and the window never speaks first.
    fn new_instance(address: &Address, first: bool) -> std::io::Result<NamedPipeServer> {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .create(address.path())
    }

    pub fn start_server(engine: Arc<Engine>, address: &Address) -> Result<()> {
        // `first_pipe_instance`: if any other program already made a pipe with this name, fail
        // instead of sharing it.
        let first = new_instance(address, true).map_err(|e| {
            Error::Remote(format!(
                "The command line can't reach this window, because its channel is already in \
                 use ({e})."
            ))
        })?;
        let address = address.clone();
        tokio::spawn(async move {
            let mut waiting = first;
            loop {
                if waiting.connect().await.is_err() {
                    // That caller went away before we answered; open a fresh end and carry on.
                    match new_instance(&address, false) {
                        Ok(fresh) => waiting = fresh,
                        Err(_) => return,
                    }
                    continue;
                }
                // Open the next waiting end BEFORE serving this caller, so a second `dinghy` is
                // never turned away while the first is being answered.
                let connected = std::mem::replace(
                    &mut waiting,
                    match new_instance(&address, false) {
                        Ok(fresh) => fresh,
                        Err(_) => return,
                    },
                );
                let engine = engine.clone();
                tokio::spawn(async move {
                    let _ = serve_connection(&engine, connected).await;
                });
            }
        });
        Ok(())
    }

    pub async fn connect(address: &Address) -> Result<Option<Stream>> {
        let give_up = Instant::now() + Duration::from_secs(2);
        loop {
            match ClientOptions::new().open(address.path()) {
                Ok(client) => return Ok(Some(client)),
                // No pipe with that name: the window isn't open.
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY) && Instant::now() < give_up => {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                    return Err(Error::Remote(
                        "BitDinghy's window is open, but this dinghy isn't allowed to talk to it. \
                         Are they running as different users, or is one of them running as \
                         administrator?"
                            .to_string(),
                    ));
                }
                Err(e) => return Err(channel_error("connecting", e)),
            }
        }
    }
}

#[cfg(unix)]
mod platform {
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Arc;

    use tokio::net::{UnixListener, UnixStream};

    use super::{Address, channel_error, serve_connection};
    use crate::engine::Engine;
    use crate::error::{Error, Result};

    pub type Stream = UnixStream;

    pub fn start_server(engine: Arc<Engine>, address: &Address) -> Result<()> {
        let path = address.path();
        if path.exists() {
            // Someone is answering there already: don't take it over.
            if std::os::unix::net::UnixStream::connect(path).is_ok() {
                return Err(Error::Remote(
                    "The command line can't reach this window, because its channel is already \
                     in use."
                        .to_string(),
                ));
            }
            // A leftover from a BitDinghy that crashed.
            let _ = std::fs::remove_file(path);
        }
        let listener = UnixListener::bind(path).map_err(|e| channel_error("opening", e))?;
        // Only this user may connect.
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| channel_error("protecting", e))?;
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let engine = engine.clone();
                tokio::spawn(async move {
                    let _ = serve_connection(&engine, stream).await;
                });
            }
        });
        Ok(())
    }

    pub async fn connect(address: &Address) -> Result<Option<Stream>> {
        match UnixStream::connect(address.path()).await {
            Ok(stream) => Ok(Some(stream)),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                ) =>
            {
                Ok(None)
            }
            Err(e) => Err(channel_error("connecting", e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::EngineConfig;

    async fn offline_engine(dir: &Path) -> Engine {
        let mut config = EngineConfig::new(dir);
        config.use_dht = false;
        config.accept_incoming = false;
        config.local_discovery = false;
        Engine::start(config).await.unwrap()
    }

    /// Talks to `serve_connection` over an in-memory pipe, line by line.
    async fn exchange(engine: &Engine, lines: &[&str]) -> Vec<String> {
        let (ours, theirs) = tokio::io::duplex(64 * 1024);
        let server = serve_connection(engine, theirs);
        let talk = async move {
            let (reader, mut writer) = tokio::io::split(ours);
            let mut reader = tokio::io::BufReader::new(reader);
            let mut answers = Vec::new();
            for line in lines {
                writer.write_all(line.as_bytes()).await.unwrap();
                writer.write_all(b"\n").await.unwrap();
                answers.push(read_line(&mut reader).await.unwrap().unwrap());
            }
            drop(writer);
            answers
        };
        let (served, answers) = tokio::join!(server, talk);
        served.unwrap();
        answers
    }

    #[test]
    fn each_data_folder_gets_its_own_channel() {
        let first = Address::for_data_dir(Path::new("C:/Apps/BitDinghy/data"));
        let again = Address::for_data_dir(Path::new("C:/Apps/BitDinghy/data"));
        let other = Address::for_data_dir(Path::new("E:/BitDinghy/data"));
        assert_eq!(first, again);
        assert_ne!(first, other);
        if cfg!(windows) {
            assert!(
                first
                    .path()
                    .to_string_lossy()
                    .starts_with(r"\\.\pipe\BitDinghy-")
            );
            // Upper or lower case, it is the same folder on Windows.
            assert_eq!(
                first,
                Address::for_data_dir(Path::new("c:/apps/bitdinghy/DATA"))
            );
        }
    }

    #[test]
    fn requests_and_replies_survive_the_trip() {
        let request = Request {
            version: PROTOCOL_VERSION,
            command: Command::Add {
                link: "magnet:?xt=urn:btih:c9e15763f722f23e98a29decdfae341b98d53056".to_string(),
                peers: vec!["127.0.0.1:6881".parse().unwrap()],
            },
        };
        let text = serde_json::to_string(&request).unwrap();
        assert!(!text.contains('\n'), "one message is one line");
        assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), request);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_window_answers_list_and_add() {
        let work = tempfile::tempdir().unwrap();
        let engine = offline_engine(work.path()).await;
        let list = format!(r#"{{"version":{PROTOCOL_VERSION},"command":{{"type":"list"}}}}"#);
        let add = format!(
            r#"{{"version":{PROTOCOL_VERSION},"command":{{"type":"add","link":"magnet:?xt=urn:btih:c9e15763f722f23e98a29decdfae341b98d53056&dn=Example","peers":[]}}}}"#
        );
        let answers = exchange(&engine, &[&list, &add, &list]).await;

        let Reply::Downloads { downloads, .. } = serde_json::from_str(&answers[0]).unwrap() else {
            panic!("{}", answers[0]);
        };
        assert!(downloads.is_empty());
        let Reply::Added { info_hash, .. } = serde_json::from_str(&answers[1]).unwrap() else {
            panic!("{}", answers[1]);
        };
        assert_eq!(info_hash, "c9e15763f722f23e98a29decdfae341b98d53056");
        let Reply::Downloads { downloads, .. } = serde_json::from_str(&answers[2]).unwrap() else {
            panic!("{}", answers[2]);
        };
        assert_eq!(downloads.len(), 1);
        assert_eq!(downloads[0].name, "Example");
        engine.shutdown().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn nonsense_and_other_versions_get_a_plain_answer_and_the_line_stays_open() {
        let work = tempfile::tempdir().unwrap();
        let engine = offline_engine(work.path()).await;
        let answers = exchange(
            &engine,
            &[
                "this is not JSON",
                r#"{"version":99,"command":{"type":"launch_rockets"}}"#,
                &format!(r#"{{"version":{PROTOCOL_VERSION},"command":{{"type":"launch_rockets"}}}}"#),
                &format!(r#"{{"version":{PROTOCOL_VERSION},"command":{{"type":"add","link":"hello","peers":[]}}}}"#),
                &format!(r#"{{"version":{PROTOCOL_VERSION},"command":{{"type":"list"}}}}"#),
            ],
        )
        .await;
        let problem = |answer: &str| match serde_json::from_str(answer).unwrap() {
            Reply::Problem { message } => message,
            other => panic!("expected a problem, got {other:?}"),
        };
        assert!(problem(&answers[0]).contains("didn't understand"));
        assert!(problem(&answers[1]).contains("different versions"));
        assert!(problem(&answers[2]).contains("didn't understand"));
        assert!(problem(&answers[3]).contains("isn't a magnet link"));
        // ...and the conversation still works afterwards.
        assert!(matches!(
            serde_json::from_str(&answers[4]).unwrap(),
            Reply::Downloads { .. }
        ));
        engine.shutdown().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_message_that_is_far_too_long_ends_the_conversation() {
        let work = tempfile::tempdir().unwrap();
        let engine = offline_engine(work.path()).await;
        let (ours, theirs) = tokio::io::duplex(64 * 1024);
        let server = serve_connection(&engine, theirs);
        let flood = async move {
            let (_reader, mut writer) = tokio::io::split(ours);
            let chunk = vec![b'a'; 64 * 1024];
            // Keep writing until the window stops reading.
            for _ in 0..(MAX_MESSAGE_BYTES / chunk.len() + 4) {
                if writer.write_all(&chunk).await.is_err() {
                    break;
                }
            }
        };
        let (served, ()) = tokio::join!(server, flood);
        assert!(served.is_err(), "the window must give up on it");
        engine.shutdown().await;
    }
}
