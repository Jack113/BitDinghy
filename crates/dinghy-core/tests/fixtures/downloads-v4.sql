-- A BitDinghy download list exactly as list version 4 saves it (milestone M10, sharing limits).
--
-- NEVER EDIT THIS FILE. Every future BitDinghy must still load it: that is the promise that an
-- upgrade never loses anyone's download list. When the list's layout changes, add an upgrade step
-- in saved_list.rs and a NEW fixture for the new version, and keep this one as it is.
--
-- The test `the_version_4_fixture_still_loads` (saved_list.rs) turns this into a real list file.

PRAGMA application_id = 1111778425;
PRAGMA user_version = 4;

CREATE TABLE downloads (
    info_hash     TEXT PRIMARY KEY NOT NULL,
    name          TEXT NOT NULL,
    magnet        TEXT NOT NULL,
    torrent       BLOB,
    download_dir  TEXT NOT NULL,
    output_folder TEXT,
    paused        INTEGER NOT NULL DEFAULT 0,
    added_at      INTEGER NOT NULL
, skipped_files TEXT NOT NULL DEFAULT '', position INTEGER NOT NULL DEFAULT 0, uploaded_bytes INTEGER NOT NULL DEFAULT 0, shared_seconds INTEGER NOT NULL DEFAULT 0, finished_at INTEGER, sharing INTEGER NOT NULL DEFAULT 0);

-- Still asking other people for its file list; nothing shared yet.
INSERT INTO downloads VALUES (
    '9f0e1d2c3b4a59687706f5e4d3c2b1a098a7b6c5',
    'Some Linux',
    'magnet:?xt=urn%3Abtih%3A9f0e1d2c3b4a59687706f5e4d3c2b1a098a7b6c5&dn=Some+Linux',
    NULL,
    'C:\Users\Someone\Downloads\BitDinghy',
    NULL,
    0,
    1791700000,
    '',
    2,
    0,
    0,
    NULL,
    0
);

-- Finished, shared 5 MB over an hour, then stopped sharing because a limit was reached.
INSERT INTO downloads VALUES (
    '0123456789abcdef0123456789abcdef01234567',
    'testdata',
    'magnet:?xt=urn%3Abtih%3A0123456789abcdef0123456789abcdef01234567',
    CAST('d4:infod4:name8:testdataee' AS BLOB),
    'C:\Users\Someone\Downloads\BitDinghy',
    'C:\Users\Someone\Downloads\BitDinghy\testdata',
    0,
    1791700100,
    '1,3',
    1,
    5000000,
    3600,
    1791700500,
    1
);
