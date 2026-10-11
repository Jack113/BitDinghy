//! A check on the source code itself: messages for people must not contain long runs of spaces.
//!
//! A long message is split over several lines with a `\` at the end of each line; Rust then drops
//! the line break and the next line's indentation. When that `\` goes missing, the message still
//! compiles but shows a gap like "at          most". That happened once (M9), so this test looks
//! for such gaps in every source file of the workspace.

use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push(path);
        }
    }
}

/// A word, six or more spaces, and another word, on a line that has a string in it.
fn has_gap(line: &str) -> bool {
    let code = line.trim_start();
    if code.starts_with("//") || !line.contains('"') {
        return false;
    }
    let bytes = code.as_bytes();
    let mut spaces = 0;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b' ' {
            spaces += 1;
            continue;
        }
        if spaces >= 6 && index > spaces && bytes[index - spaces - 1] != b'"' && *byte != b'/' {
            return true;
        }
        spaces = 0;
    }
    false
}

#[test]
fn messages_have_no_gaps_from_a_missing_line_join() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut files = Vec::new();
    rust_files(crates, &mut files);
    assert!(files.len() > 10, "found the source files: {files:?}");
    let mut gaps = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        for (number, line) in text.lines().enumerate() {
            if has_gap(line) {
                gaps.push(format!(
                    "{}:{}: {}",
                    file.display(),
                    number + 1,
                    line.trim()
                ));
            }
        }
    }
    assert!(gaps.is_empty(), "messages with a gap:\n{}", gaps.join("\n"));
}

#[test]
fn the_gap_finder_finds_gaps() {
    let gap = " ".repeat(10);
    assert!(has_gap(&format!("        \"and at{gap}most 5\",")));
    assert!(!has_gap("        \"and at most 5\","));
    assert!(!has_gap(&format!("        // a comment{gap}with spaces")));
    assert!(!has_gap(&format!("let x = 1;{gap}// no string here")));
}
