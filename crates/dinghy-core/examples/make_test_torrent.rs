//! Makes small test torrents for trying BitDinghy out by hand (`docs/TESTING.md`): one with several
//! files ("choose files", M6, and "check files", M7), three little ones for the queue (M8), and
//! one that claims a 5 GiB file, for the FAT32 check (M12).
//! Developer tool only: it is not part of BitDinghy.exe or dinghy.exe.
//!
//!     cargo run -p dinghy-core --example make_test_torrent
//!
//! It writes some made-up files into a folder in your temp folder and a `.torrent` file next to
//! them, and prints where. Nobody shares this torrent, so adding it to BitDinghy downloads
//! nothing: the window still knows its files at once (they are in the `.torrent`), which is all
//! the choose-files test needs. Running it again makes the same torrent.

use std::path::Path;

use librqbit::spawn_utils::BlockingSpawner;
use librqbit::{CreateTorrentOptions, create_torrent};

/// The test files: (path inside the torrent's folder, size in bytes).
const FILES: &[(&str, usize)] = &[
    ("Episode 1.bin", 3_000_000),
    ("Episode 2.bin", 2_000_000),
    ("Extras/Making of.bin", 1_500_000),
    ("Extras/Poster.txt", 40),
];

const NAME: &str = "BitDinghy M6 test";

/// The same made-up bytes every time (so the torrent is always the same), not all zeros.
fn made_up_bytes(len: usize, seed: u32) -> Vec<u8> {
    let mut x: u32 = 0x9E37_79B9 ^ seed;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            (x >> 8) as u8
        })
        .collect()
}

/// Writes `files` (path, size) into `source` and makes `torrent_file` from them, named `name`.
async fn make(source: &Path, name: &str, files: &[(&str, usize)], seed: u32, torrent_file: &Path) {
    for (number, (path, size)) in files.iter().enumerate() {
        let file = source.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).expect("can't create the test folder");
        std::fs::write(&file, made_up_bytes(*size, seed + number as u32))
            .expect("can't write a test file");
    }
    let created = create_torrent(
        source,
        CreateTorrentOptions {
            name: Some(name),
            trackers: Vec::new(),
            piece_length: Some(256 * 1024),
        },
        &BlockingSpawner::new(1),
    )
    .await
    .expect("can't create the torrent");
    std::fs::write(torrent_file, created.as_bytes().expect("torrent bytes"))
        .expect("can't write the .torrent file");
}

#[tokio::main]
async fn main() {
    let place = std::env::temp_dir().join("bitdinghy-m6-test");
    let source = place.join(NAME);
    let torrent_file = place.join("bitdinghy-m6-test.torrent");
    make(&source, NAME, FILES, 0, &torrent_file).await;
    println!("Test torrent: {}", torrent_file.display());
    println!(
        "It holds {} made-up files, kept in {}.",
        FILES.len(),
        source.display()
    );
    println!("Nobody shares it, so adding it to BitDinghy downloads nothing.");

    // Three small ones for the queue. Their made-up files stay here, never in Downloads, so a
    // download of them simply waits at 0%.
    for number in 1..=3 {
        let name = format!("Queue test {number}");
        let files = [("part 1.bin", 300_000), ("part 2.bin", 300_000)];
        let torrent_file = place.join(format!("queue-{number}.torrent"));
        make(
            &place.join("queue-sources").join(&name),
            &name,
            &files,
            100 * number,
            &torrent_file,
        )
        .await;
    }
    println!(
        "Queue test torrents: queue-1.torrent, queue-2.torrent and queue-3.torrent in {}",
        place.display()
    );

    // One file of 5 GiB, written by hand (its piece fingerprints are made up, so it can never
    // download anything): for the FAT32 check, which refuses it before writing a byte (M12).
    let size: u64 = 5 * 1024 * 1024 * 1024;
    let piece: u64 = 64 * 1024 * 1024;
    let pieces = vec![0x5Au8; (size.div_ceil(piece) * 20) as usize];
    let mut big = format!(
        "d4:infod6:lengthi{size}e4:name7:big.iso12:piece lengthi{piece}e6:pieces{}:",
        pieces.len()
    )
    .into_bytes();
    big.extend_from_slice(&pieces);
    big.extend_from_slice(b"ee");
    let big_file = place.join("big-file-test.torrent");
    std::fs::write(&big_file, big).expect("can't write the .torrent file");
    println!(
        "Big-file test torrent (one 5 GiB file, nothing to download): {}",
        big_file.display()
    );
}
