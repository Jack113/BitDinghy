//! Makes a small test torrent with several files, for trying out "choose files" by hand
//! (`docs/TESTING.md`, M6). Developer tool only: it is not part of BitDinghy.exe or dinghy.exe.
//!
//!     cargo run -p dinghy-core --example make_test_torrent
//!
//! It writes some made-up files into a folder in your temp folder and a `.torrent` file next to
//! them, and prints where. Nobody shares this torrent, so adding it to BitDinghy downloads
//! nothing: the window still knows its files at once (they are in the `.torrent`), which is all
//! the choose-files test needs. Running it again makes the same torrent.

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

#[tokio::main]
async fn main() {
    let place = std::env::temp_dir().join("bitdinghy-m6-test");
    let source = place.join(NAME);
    for (seed, (path, size)) in FILES.iter().enumerate() {
        let file = source.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).expect("can't create the test folder");
        std::fs::write(&file, made_up_bytes(*size, seed as u32)).expect("can't write a test file");
    }

    let created = create_torrent(
        &source,
        CreateTorrentOptions {
            name: Some(NAME),
            trackers: Vec::new(),
            piece_length: Some(256 * 1024),
        },
        &BlockingSpawner::new(1),
    )
    .await
    .expect("can't create the torrent");
    let torrent_file = place.join("bitdinghy-m6-test.torrent");
    std::fs::write(&torrent_file, created.as_bytes().expect("torrent bytes"))
        .expect("can't write the .torrent file");

    println!("Test torrent: {}", torrent_file.display());
    println!(
        "It holds {} made-up files, kept in {}.",
        FILES.len(),
        source.display()
    );
    println!("Nobody shares it, so adding it to BitDinghy downloads nothing.");
}
