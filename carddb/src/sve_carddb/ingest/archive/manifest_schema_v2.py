"""Frozen offline-import schema; live v1 DDL changes cannot redefine sealed v2."""

SCHEMA_SQL = """
CREATE TABLE IF NOT EXISTS resource (
    url               TEXT PRIMARY KEY,
    region            TEXT NOT NULL,
    kind              TEXT NOT NULL,
    path              TEXT NOT NULL UNIQUE,
    sha256            TEXT NOT NULL,
    raw_bytes         INTEGER NOT NULL,
    stored_bytes      INTEGER NOT NULL,
    content_type      TEXT NOT NULL,
    etag              TEXT,
    last_modified     TEXT,
    first_fetched_at  TEXT NOT NULL,
    last_checked_at   TEXT NOT NULL,
    last_changed_at   TEXT NOT NULL,
    archived_at       TEXT
) STRICT;

CREATE TABLE IF NOT EXISTS fetch_log (
    id                     INTEGER PRIMARY KEY,
    run_id                 TEXT NOT NULL,
    logical_fetch_id       TEXT NOT NULL,
    attempt                INTEGER NOT NULL,
    hop                    INTEGER NOT NULL,
    url                    TEXT NOT NULL,
    requested_url          TEXT NOT NULL,
    final_url              TEXT,
    started_at             TEXT NOT NULL,
    finished_at            TEXT,
    elapsed_ms             INTEGER,
    status                 INTEGER,
    error_class            TEXT,
    sent_if_none_match     TEXT,
    response_sha256        TEXT,
    response_bytes         INTEGER,
    response_etag          TEXT,
    response_last_modified TEXT,
    outcome                TEXT NOT NULL,
    validation_error       TEXT
) STRICT;

CREATE INDEX IF NOT EXISTS fetch_log_url ON fetch_log (url);

-- Current links of single-page sources (card, limit, errata, news, rules).
CREATE TABLE IF NOT EXISTS link (
    from_url     TEXT NOT NULL,
    to_url       TEXT NOT NULL,
    to_kind      TEXT NOT NULL,
    position     INTEGER NOT NULL,
    original     TEXT NOT NULL,
    from_sha256  TEXT NOT NULL,
    PRIMARY KEY (from_url, to_kind, position)
) STRICT;

CREATE TABLE IF NOT EXISTS link_log (
    id           INTEGER PRIMARY KEY,
    from_url     TEXT NOT NULL,
    to_url       TEXT NOT NULL,
    to_kind      TEXT NOT NULL,
    position     INTEGER NOT NULL,
    original     TEXT NOT NULL,
    from_sha256  TEXT NOT NULL,
    event        TEXT NOT NULL,
    at           TEXT NOT NULL
) STRICT;

-- Multi-page discoveries (sets, list per set, errata index, paged Q&A).
-- Each generation keeps its own immutable snapshot of pages and edges.
CREATE TABLE IF NOT EXISTS discovery_generation (
    id              INTEGER PRIMARY KEY,
    root            TEXT NOT NULL,
    status          TEXT NOT NULL,
    started_at      TEXT NOT NULL,
    finished_at     TEXT,
    declared_total  INTEGER,
    max_page        INTEGER
) STRICT;

CREATE UNIQUE INDEX IF NOT EXISTS one_validated_generation_per_root
    ON discovery_generation (root) WHERE status = 'validated';

CREATE TABLE IF NOT EXISTS generation_page (
    generation_id  INTEGER NOT NULL REFERENCES discovery_generation (id),
    page_url       TEXT NOT NULL,
    page_sha256    TEXT NOT NULL,
    PRIMARY KEY (generation_id, page_url)
) STRICT;

CREATE TABLE IF NOT EXISTS generation_edge (
    generation_id  INTEGER NOT NULL,
    from_url       TEXT NOT NULL,
    to_url         TEXT NOT NULL,
    to_kind        TEXT NOT NULL,
    position       INTEGER NOT NULL,
    original       TEXT NOT NULL,
    PRIMARY KEY (generation_id, from_url, to_kind, position),
    FOREIGN KEY (generation_id, from_url)
        REFERENCES generation_page (generation_id, page_url)
) STRICT;

CREATE TABLE source_import_receipt (
    receipt_id     TEXT PRIMARY KEY,
    index_bytes    BLOB NOT NULL,
    index_sha256   TEXT NOT NULL,
    content        BLOB NOT NULL,
    registered_at  TEXT NOT NULL
) STRICT;
"""
