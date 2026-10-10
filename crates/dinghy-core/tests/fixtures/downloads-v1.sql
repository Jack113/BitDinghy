-- A BitDinghy download list exactly as list version 1 saves it (BitDinghy 0.0.1, milestone M3).
--
-- NEVER EDIT THIS FILE. Every future BitDinghy must still load it: that is the promise that an
-- upgrade never loses anyone's download list. When the list's layout changes, add an upgrade step
-- in saved_list.rs and a NEW fixture for the new version, and keep this one as it is.
--
-- The test `the_version_1_fixture_still_loads` (saved_list.rs) turns this into a real list file.

PRAGMA application_id = 1111778425;
PRAGMA user_version = 1;

CREATE TABLE downloads (
    info_hash     TEXT PRIMARY KEY NOT NULL,
    name          TEXT NOT NULL,
    magnet        TEXT NOT NULL,
    torrent       BLOB,
    download_dir  TEXT NOT NULL,
    output_folder TEXT,
    paused        INTEGER NOT NULL DEFAULT 0,
    added_at      INTEGER NOT NULL
);

-- Still asking other people for its file list: no torrent, no folder yet.
INSERT INTO downloads VALUES (
    'c9e15763f722f23e98a29decdfae341b98d53056',
    'Ubuntu 24.04',
    'magnet:?xt=urn%3Abtih%3Ac9e15763f722f23e98a29decdfae341b98d53056&dn=Ubuntu+24.04',
    NULL,
    'C:\Users\Someone\Downloads\BitDinghy',
    NULL,
    0,
    1791620000
);

-- File list known and checked, and paused by the person.
INSERT INTO downloads VALUES (
    '0123456789abcdef0123456789abcdef01234567',
    'testdata',
    'magnet:?xt=urn%3Abtih%3A0123456789abcdef0123456789abcdef01234567',
    CAST('d4:infod4:name8:testdataee' AS BLOB),
    'C:\Users\Someone\Downloads\BitDinghy',
    'C:\Users\Someone\Downloads\BitDinghy\testdata',
    1,
    1791620100
);
