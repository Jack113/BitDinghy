-- A BitDinghy download list exactly as list version 5 saves it (milestone M14, history).
--
-- NEVER EDIT THIS FILE. Every future BitDinghy must still load it: that is the promise that an
-- upgrade never loses anyone's download list. When the list's layout changes, add an upgrade step
-- in saved_list.rs and a NEW fixture for the new version, and keep this one as it is.
--
-- The test `the_version_5_fixture_still_loads` (saved_list.rs) turns this into a real list file.

PRAGMA application_id = 1111778425;
PRAGMA user_version = 5;

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

CREATE TABLE history (
    info_hash      TEXT NOT NULL,
    name           TEXT NOT NULL,
    size_bytes     INTEGER NOT NULL,
    added_at       INTEGER NOT NULL,
    finished_at    INTEGER,
    removed_at     INTEGER NOT NULL,
    uploaded_bytes INTEGER NOT NULL,
    folder         TEXT NOT NULL
);

CREATE TABLE daily_totals (
    day              INTEGER PRIMARY KEY NOT NULL,
    downloaded_bytes INTEGER NOT NULL DEFAULT 0,
    uploaded_bytes   INTEGER NOT NULL DEFAULT 0
);

-- Still in the list.
INSERT INTO downloads VALUES (
    '0123456789abcdef0123456789abcdef01234567',
    'testdata',
    'magnet:?xt=urn%3Abtih%3A0123456789abcdef0123456789abcdef01234567',
    CAST('d4:infod4:name8:testdataee' AS BLOB),
    'C:\Users\Someone\Downloads\BitDinghy',
    'C:\Users\Someone\Downloads\BitDinghy\testdata',
    0,
    1791700100,
    '',
    1,
    5000000,
    3600,
    1791700500,
    0
);

-- Removed after it finished.
INSERT INTO history VALUES (
    '9f0e1d2c3b4a59687706f5e4d3c2b1a098a7b6c5',
    'Old Linux',
    1500000000,
    1791700000,
    1791703600,
    1791800000,
    250000000,
    'C:\Users\Someone\Downloads\BitDinghy'
);

-- One day of totals.
INSERT INTO daily_totals VALUES (20737, 1500000000, 250000000);
