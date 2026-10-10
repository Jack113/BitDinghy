-- A BitDinghy download list exactly as list version 2 saves it (milestone M6, choosing files).
--
-- NEVER EDIT THIS FILE. Every future BitDinghy must still load it: that is the promise that an
-- upgrade never loses anyone's download list. When the list's layout changes, add an upgrade step
-- in saved_list.rs and a NEW fixture for the new version, and keep this one as it is.
--
-- The test `the_version_2_fixture_still_loads` (saved_list.rs) turns this into a real list file.

PRAGMA application_id = 1111778425;
PRAGMA user_version = 2;

CREATE TABLE downloads (
    info_hash     TEXT PRIMARY KEY NOT NULL,
    name          TEXT NOT NULL,
    magnet        TEXT NOT NULL,
    torrent       BLOB,
    download_dir  TEXT NOT NULL,
    output_folder TEXT,
    paused        INTEGER NOT NULL DEFAULT 0,
    added_at      INTEGER NOT NULL
, skipped_files TEXT NOT NULL DEFAULT '');

-- Still asking other people for its file list: every file wanted (nothing skipped).
INSERT INTO downloads VALUES (
    '9f0e1d2c3b4a59687706f5e4d3c2b1a098a7b6c5',
    'Some Linux',
    'magnet:?xt=urn%3Abtih%3A9f0e1d2c3b4a59687706f5e4d3c2b1a098a7b6c5&dn=Some+Linux',
    NULL,
    'C:\Users\Someone\Downloads\BitDinghy',
    NULL,
    0,
    1791700000,
    ''
);

-- File list known; the person unticked the files with indexes 1 and 3.
INSERT INTO downloads VALUES (
    '0123456789abcdef0123456789abcdef01234567',
    'testdata',
    'magnet:?xt=urn%3Abtih%3A0123456789abcdef0123456789abcdef01234567',
    CAST('d4:infod4:name8:testdataee' AS BLOB),
    'C:\Users\Someone\Downloads\BitDinghy',
    'C:\Users\Someone\Downloads\BitDinghy\testdata',
    1,
    1791700100,
    '1,3'
);
