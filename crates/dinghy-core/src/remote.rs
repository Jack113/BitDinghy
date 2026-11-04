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

use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt,
};

use crate::engine::{AddOptions, DownloadSnapshot, Engine, Move};
use crate::error::{Error, Result};
use crate::file_choice::FileChoice;

/// The version of the messages below. Change it whenever a message changes shape.
/// Version 2 (M6 and later, not released before): choosing files (`Files`, `ChooseFiles`), file
/// counts in each download, `Check`, `use_existing_files` on `Add`, `Move`, `SetMaxActiveDownloads`
/// and the `Queued` status.
pub const PROTOCOL_VERSION: u32 = 2;

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
    /// Add a magnet link, a `.torrent` file (by its full path on this computer) or a web link to
    /// one (the window fetches it, with its own http setting), exactly as if
    /// it was pasted into the window. `peers` are extra addresses to try (`dinghy get --peer`).
    Add {
        link: String,
        peers: Vec<SocketAddr>,
        /// Use files with the same names already in the download folder (`--use-existing-files`).
        #[serde(default)]
        use_existing_files: bool,
    },
    Pause {
        target: Target,
    },
    Resume {
        target: Target,
    },
    /// Which files "remove, delete the files too" would delete right now. Changes nothing.
    FilesToDelete {
        target: Target,
    },
    Remove {
        target: Target,
        delete_files: bool,
    },
    /// A download's files, with which are chosen.
    Files {
        target: Target,
    },
    /// Download only these files (their numbers from `Files`, starting at 1).
    ChooseFiles {
        target: Target,
        chosen: BTreeSet<usize>,
    },
    /// Check a download's files on disk again (the result shows up in its `note`).
    Check {
        target: Target,
    },
    /// Move a download in the list (the queue order).
    Move {
        target: Target,
        to: Move,
    },
    /// Change how many downloads run at the same time, for this run (the CLI saves the setting).
    SetMaxActiveDownloads {
        limit: usize,
    },
    /// Change the "allow plain http links" setting, for this run (the CLI saves the setting).
    SetAllowHttpLinks {
        allow: bool,
    },
    /// Change what happens when a download is finished, for this run (the CLI saves the setting).
    SetWhenFinished {
        choice: crate::settings::WhenFinished,
    },
    /// A link another program asked BitDinghy to open (a browser or Explorer started a second
    /// BitDinghy.exe with it): the window asks the person before adding it.
    Offer {
        link: String,
    },
    /// Change the sharing limits (ratio in percent, minutes; 0: none), for this run.
    SetSharingLimits {
        ratio_percent: u32,
        minutes: u32,
    },
    /// Change the speed limits, in KiB/s (0: no limit), for this run (the CLI saves them).
    SetSpeedLimits {
        download_kib: u32,
        upload_kib: u32,
    },
}

/// Which download a request is about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    /// Its number in the window's list, as `dinghy list` shows it (starting at 1).
    Number(usize),
    /// Its fingerprint. Used for the actual removal after a confirmation, so a list that changed
    /// in between can never make it remove a different download.
    InfoHash(String),
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
    /// Paused, resumed, or its files chosen.
    Changed { name: String },
    /// The files that would be deleted (they exist right now).
    Files {
        info_hash: String,
        name: String,
        paths: Vec<PathBuf>,
    },
    /// Removed. `folder` is where its files are (or were); `deleted` and `problems` say what
    /// deleting them did.
    Removed {
        name: String,
        folder: PathBuf,
        deleted: usize,
        problems: Vec<String>,
    },
    /// A download's files, with which are chosen.
    FileList {
        info_hash: String,
        name: String,
        files: Vec<FileChoice>,
    },
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

/// Finds the download a request is about, in the window's list as it is right now.
fn find(engine: &Engine, target: &Target) -> std::result::Result<DownloadSnapshot, String> {
    let downloads = engine.snapshot();
    let found = match target {
        Target::Number(number) => number
            .checked_sub(1)
            .and_then(|index| downloads.into_iter().nth(index)),
        Target::InfoHash(info_hash) => downloads
            .into_iter()
            .find(|download| download.info_hash == *info_hash),
    };
    found.ok_or_else(|| match target {
        Target::Number(number) => format!(
            "There is no download number {number} in BitDinghy's window. `dinghy list` shows \
             the numbers."
        ),
        Target::InfoHash(_) => "That download isn't in BitDinghy's window any more.".to_string(),
    })
}

/// Answers one request. Any problem becomes a `Reply::Problem` with a message for the person.
async fn answer(engine: &Engine, command: Command) -> Reply {
    answer_or_problem(engine, command)
        .await
        .unwrap_or_else(|message| Reply::Problem { message })
}

/// `?` below means "stop here and answer with this problem".
async fn answer_or_problem(
    engine: &Engine,
    command: Command,
) -> std::result::Result<Reply, String> {
    let said = |error: Error| error.to_string();
    Ok(match command {
        Command::List => Reply::Downloads {
            downloads: engine.snapshot(),
            download_dir: engine.download_dir().to_path_buf(),
        },
        Command::Add {
            link,
            peers,
            use_existing_files,
        } => {
            let options = AddOptions {
                peers,
                use_existing_files,
            };
            let id = engine.add_any_with(&link, options).await.map_err(said)?;
            Reply::Added {
                info_hash: engine
                    .snapshot()
                    .into_iter()
                    .find(|download| download.id == id)
                    .map(|download| download.info_hash)
                    .unwrap_or_default(),
                folder: engine
                    .download_dir_of(id)
                    .unwrap_or_else(|| engine.download_dir().to_path_buf()),
            }
        }
        Command::Pause { target } => {
            let download = find(engine, &target)?;
            engine.pause(download.id).await.map_err(said)?;
            Reply::Changed {
                name: download.name,
            }
        }
        Command::Resume { target } => {
            let download = find(engine, &target)?;
            engine.resume(download.id).await.map_err(said)?;
            Reply::Changed {
                name: download.name,
            }
        }
        Command::FilesToDelete { target } => {
            let download = find(engine, &target)?;
            Reply::Files {
                paths: engine.files_to_delete(download.id).map_err(said)?,
                info_hash: download.info_hash,
                name: download.name,
            }
        }
        Command::Remove {
            target,
            delete_files,
        } => {
            let download = find(engine, &target)?;
            let folder = download
                .save_folder
                .clone()
                .or_else(|| engine.download_dir_of(download.id))
                .unwrap_or_default();
            let deleted = engine
                .remove(download.id, delete_files)
                .await
                .map_err(said)?;
            Reply::Removed {
                name: download.name,
                folder,
                deleted: deleted.files,
                problems: deleted.problems,
            }
        }
        Command::Files { target } => {
            let download = find(engine, &target)?;
            Reply::FileList {
                files: engine.files(download.id).map_err(said)?,
                info_hash: download.info_hash,
                name: download.name,
            }
        }
        Command::Move { target, to } => {
            let download = find(engine, &target)?;
            engine.move_download(download.id, to).map_err(said)?;
            Reply::Changed {
                name: download.name,
            }
        }
        Command::SetMaxActiveDownloads { limit } => {
            engine.set_max_active_downloads(limit);
            Reply::Changed {
                name: String::new(),
            }
        }
        Command::Offer { link } => {
            // Checked like anything typed, so a bad link is refused now, not after asking.
            crate::input::classify(&link).map_err(said)?;
            engine.offer(link);
            Reply::Changed {
                name: String::new(),
            }
        }
        Command::SetSharingLimits {
            ratio_percent,
            minutes,
        } => {
            engine.set_sharing_limits(ratio_percent, minutes);
            Reply::Changed {
                name: String::new(),
            }
        }
        Command::SetSpeedLimits {
            download_kib,
            upload_kib,
        } => {
            for (which, kib) in [("download", download_kib), ("upload", upload_kib)] {
                crate::profile::check_speed_limit(which, kib).map_err(said)?;
            }
            engine.set_speed_limits(download_kib, upload_kib);
            Reply::Changed {
                name: String::new(),
            }
        }
        Command::SetAllowHttpLinks { allow } => {
            engine.set_allow_http_links(allow);
            Reply::Changed {
                name: String::new(),
            }
        }
        Command::SetWhenFinished { choice } => {
            engine.set_when_finished(choice);
            Reply::Changed {
                name: String::new(),
            }
        }
        Command::Check { target } => {
            let download = find(engine, &target)?;
            engine.check(download.id).await.map_err(said)?;
            Reply::Changed {
                name: download.name,
            }
        }
        Command::ChooseFiles { target, chosen } => {
            let download = find(engine, &target)?;
            engine
                .choose_files(download.id, &chosen)
                .await
                .map_err(said)?;
            Reply::Changed {
                name: download.name,
            }
        }
    })
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
                Ok(client) => {
                    check_owner(&client)?;
                    return Ok(Some(client));
                }
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

    /// Makes sure the pipe was made by this same Windows user. The pipe's name isn't secret, so
    /// another user of this PC could make a pipe with BitDinghy's name before the window opens;
    /// `dinghy` would then send its links to that program and believe its answers. A pipe's
    /// owner is the user who made it (or, for a program run as administrator, the
    /// Administrators group: the same as this program's own "default owner" when it is run the
    /// same way).
    fn check_owner(client: &NamedPipeClient) -> Result<()> {
        use std::os::windows::io::AsRawHandle;
        let ours = this_user().map_err(|e| channel_error("checking who made the channel", e))?;
        let owner = owner_of(client.as_raw_handle())
            .map_err(|e| channel_error("checking who made the channel", e))?;
        if is_ours(&owner, &ours) {
            Ok(())
        } else {
            Err(Error::Remote(
                "Something that isn't BitDinghy's window, started by another user of this PC, \
                 is using BitDinghy's channel, so dinghy won't talk to it. Restarting the PC \
                 clears it."
                    .to_string(),
            ))
        }
    }

    /// True when `owner` is one of `ours`.
    pub(super) fn is_ours(owner: &[u8], ours: &[Vec<u8>]) -> bool {
        ours.iter().any(|sid| sid.as_slice() == owner)
    }

    /// A security identifier (SID) copied into plain bytes. Two SIDs are the same when their
    /// bytes are the same.
    unsafe fn sid_bytes(sid: *mut core::ffi::c_void) -> Vec<u8> {
        use windows_sys::Win32::Security::GetLengthSid;
        // SAFETY: the caller passes a valid SID; `GetLengthSid` gives its exact size.
        unsafe {
            let length = GetLengthSid(sid) as usize;
            std::slice::from_raw_parts(sid as *const u8, length).to_vec()
        }
    }

    /// The owner of the object behind `handle`.
    pub(super) fn owner_of(handle: std::os::windows::io::RawHandle) -> std::io::Result<Vec<u8>> {
        use windows_sys::Win32::Foundation::LocalFree;
        use windows_sys::Win32::Security::Authorization::{GetSecurityInfo, SE_KERNEL_OBJECT};
        use windows_sys::Win32::Security::OWNER_SECURITY_INFORMATION;
        let mut owner = std::ptr::null_mut();
        let mut descriptor = std::ptr::null_mut();
        // SAFETY: `handle` is an open pipe handle (opened for reading, which includes the right
        // to read its owner); Windows allocates the descriptor, which is freed below.
        unsafe {
            let status = GetSecurityInfo(
                handle,
                SE_KERNEL_OBJECT,
                OWNER_SECURITY_INFORMATION,
                &mut owner,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut descriptor,
            );
            if status != 0 {
                return Err(std::io::Error::from_raw_os_error(status as i32));
            }
            let bytes = sid_bytes(owner);
            LocalFree(descriptor);
            Ok(bytes)
        }
    }

    /// This program's user, and the owner it gives to things it makes (its "default owner",
    /// which is the Administrators group when it runs as administrator).
    pub(super) fn this_user() -> std::io::Result<Vec<Vec<u8>>> {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::Security::{
            GetTokenInformation, TOKEN_OWNER, TOKEN_QUERY, TOKEN_USER, TokenOwner, TokenUser,
        };
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
        // SAFETY: plain Windows calls on this process's own token; buffers are sized by asking
        // Windows first, and the token handle is closed before returning.
        unsafe {
            let mut token = std::ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return Err(std::io::Error::last_os_error());
            }
            let read = |class| -> std::io::Result<Vec<u64>> {
                let mut needed = 0u32;
                GetTokenInformation(token, class, std::ptr::null_mut(), 0, &mut needed);
                // u64 parts keep the buffer aligned for the structure Windows writes into it.
                let mut buffer = vec![0u64; (needed as usize).div_ceil(8)];
                if GetTokenInformation(
                    token,
                    class,
                    buffer.as_mut_ptr().cast(),
                    needed,
                    &mut needed,
                ) == 0
                {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(buffer)
            };
            let result = (|| {
                let user = read(TokenUser)?;
                let owner = read(TokenOwner)?;
                Ok(vec![
                    sid_bytes((*user.as_ptr().cast::<TOKEN_USER>()).User.Sid),
                    sid_bytes((*owner.as_ptr().cast::<TOKEN_OWNER>()).Owner),
                ])
            })();
            CloseHandle(token);
            result
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
        let cant_open = |e: std::io::Error| {
            Error::Remote(format!(
                "The command line can't reach this window, because its channel couldn't be \
                 opened ({e})."
            ))
        };
        // The socket file lives in the data folder, which normally exists already.
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder).map_err(cant_open)?;
        }
        let listener = UnixListener::bind(path).map_err(cant_open)?;
        // Only this user may connect.
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(cant_open)?;
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
                use_existing_files: false,
            },
        };
        let text = serde_json::to_string(&request).unwrap();
        assert!(!text.contains('\n'), "one message is one line");
        assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), request);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn dinghy_talks_to_a_pipe_made_by_the_same_user() {
        use tokio::net::windows::named_pipe::ServerOptions;
        let work = tempfile::tempdir().unwrap();
        let address = Address::for_data_dir(work.path());
        let _server = ServerOptions::new()
            .first_pipe_instance(true)
            .create(address.path())
            .unwrap();
        assert!(platform::connect(&address).await.unwrap().is_some());
    }

    #[cfg(windows)]
    #[test]
    fn a_pipe_owned_by_someone_else_is_not_ours() {
        let ours = platform::this_user().unwrap();
        assert_eq!(ours.len(), 2);
        assert!(platform::is_ours(&ours[0], &ours));
        // S-1-5-18, the "Local System" account: never the person running the tests.
        let system = [1u8, 1, 0, 0, 0, 0, 0, 5, 18, 0, 0, 0];
        assert!(!ours.contains(&system.to_vec()));
        assert!(!platform::is_ours(&system, &ours));
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
    async fn pause_resume_and_remove_find_the_download_by_number_or_fingerprint() {
        let work = tempfile::tempdir().unwrap();
        let engine = offline_engine(work.path()).await;
        let hash = "0123456789abcdef0123456789abcdef01234567";
        engine
            .add_magnet(&format!("magnet:?xt=urn:btih:{hash}&dn=Example"), vec![])
            .unwrap();
        let request = |command: Command| {
            serde_json::to_string(&Request {
                version: PROTOCOL_VERSION,
                command,
            })
            .unwrap()
        };
        let lines = [
            request(Command::Pause {
                target: Target::Number(1),
            }),
            request(Command::Resume {
                target: Target::Number(1),
            }),
            request(Command::Pause {
                target: Target::Number(5),
            }),
            request(Command::FilesToDelete {
                target: Target::Number(1),
            }),
            request(Command::Files {
                target: Target::Number(1),
            }),
            request(Command::ChooseFiles {
                target: Target::Number(1),
                chosen: BTreeSet::from([1]),
            }),
            request(Command::Check {
                target: Target::Number(1),
            }),
            request(Command::Remove {
                target: Target::InfoHash(hash.to_string()),
                delete_files: true,
            }),
            request(Command::Remove {
                target: Target::InfoHash(hash.to_string()),
                delete_files: false,
            }),
        ];
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        let answers: Vec<Reply> = exchange(&engine, &lines)
            .await
            .iter()
            .map(|answer| serde_json::from_str(answer).unwrap())
            .collect();

        assert_eq!(
            answers[0],
            Reply::Changed {
                name: "Example".into()
            }
        );
        assert_eq!(
            answers[1],
            Reply::Changed {
                name: "Example".into()
            }
        );
        assert!(
            matches!(&answers[2], Reply::Problem { message } if message.contains("no download number 5")),
            "{:?}",
            answers[2]
        );
        // Its file list never arrived, so there is nothing to delete.
        assert_eq!(
            answers[3],
            Reply::Files {
                info_hash: hash.to_string(),
                name: "Example".into(),
                paths: vec![]
            }
        );
        // ...and no files to show or choose yet.
        for answer in &answers[4..6] {
            assert!(
                matches!(answer, Reply::Problem { message } if message.contains("doesn't know this download's files yet")),
                "{answer:?}"
            );
        }
        // ...and nothing to check yet either.
        assert!(
            matches!(&answers[6], Reply::Problem { message } if message.contains("can't be checked yet")),
            "{:?}",
            answers[6]
        );
        assert!(
            matches!(&answers[7], Reply::Removed { name, deleted: 0, .. } if name == "Example"),
            "{:?}",
            answers[7]
        );
        // Removed by fingerprint once; a second time there is nothing left to remove.
        assert!(
            matches!(&answers[8], Reply::Problem { message } if message.contains("isn't in BitDinghy's window")),
            "{:?}",
            answers[8]
        );
        assert!(engine.snapshot().is_empty());
        engine.shutdown().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_offered_link_waits_for_the_person_and_a_bad_one_is_refused() {
        let work = tempfile::tempdir().unwrap();
        let engine = offline_engine(work.path()).await;
        let request = |link: &str| {
            serde_json::to_string(&Request {
                version: PROTOCOL_VERSION,
                command: Command::Offer {
                    link: link.to_string(),
                },
            })
            .unwrap()
        };
        let good = "magnet:?xt=urn:btih:5a8b2f0c9e1d4b7a6c3e8f0a1b2c3d4e5f6a7b8c";
        let lines = [request(good), request("magnet:?nonsense")];
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        let answers: Vec<Reply> = exchange(&engine, &lines)
            .await
            .iter()
            .map(|answer| serde_json::from_str(answer).unwrap())
            .collect();
        assert!(
            matches!(answers[0], Reply::Changed { .. }),
            "{:?}",
            answers[0]
        );
        assert!(
            matches!(answers[1], Reply::Problem { .. }),
            "{:?}",
            answers[1]
        );
        // Nothing was added: the window asks first.
        assert!(engine.snapshot().is_empty());
        assert_eq!(engine.take_offers(), vec![good.to_string()]);
        assert!(engine.take_offers().is_empty(), "taken once");
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
