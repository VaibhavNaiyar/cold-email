CREATE TYPE signal_kind AS ENUM ('raised', 'launched', 'hiring', 'repo');
CREATE TYPE contact_verification AS ENUM ('valid', 'accept_all', 'unknown', 'invalid');
CREATE TYPE draft_status AS ENUM ('pending', 'queued', 'skipped', 'sent', 'replied', 'bounced');

CREATE TABLE companies (
    id                     BIGSERIAL PRIMARY KEY,
    domain                 TEXT UNIQUE,
    name                   TEXT NOT NULL,
    name_key               TEXT NOT NULL,
    legal_name             TEXT,
    one_liner              TEXT,
    website                TEXT,
    domain_source          TEXT,
    domain_attempts        INTEGER NOT NULL DEFAULT 0,
    domain_last_attempt_at TIMESTAMPTZ,
    is_ai                  BOOLEAN,
    stage                  TEXT,
    region                 TEXT,
    remote                 BOOLEAN,
    hiring                 BOOLEAN,
    score                  INTEGER,
    score_reason           TEXT,
    first_seen_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_signal_at         TIMESTAMPTZ,
    status                 TEXT NOT NULL DEFAULT 'new'
);

-- Companies are matched by domain; name only dedupes those still waiting for one.
CREATE UNIQUE INDEX companies_unresolved_name_idx ON companies (name_key) WHERE domain IS NULL;

CREATE TABLE signals (
    id           BIGSERIAL PRIMARY KEY,
    company_id   BIGINT NOT NULL REFERENCES companies (id),
    source_type  TEXT NOT NULL,
    source_url   TEXT NOT NULL UNIQUE,
    title        TEXT NOT NULL,
    published_at TIMESTAMPTZ NOT NULL,
    kind         signal_kind NOT NULL,
    amount_usd   BIGINT,
    raw_json     JSONB
);

CREATE INDEX signals_company_id_idx ON signals (company_id);

CREATE TABLE contacts (
    id           BIGSERIAL PRIMARY KEY,
    company_id   BIGINT NOT NULL REFERENCES companies (id),
    full_name    TEXT,
    role         TEXT,
    is_technical BOOLEAN NOT NULL DEFAULT false,
    email        TEXT NOT NULL,
    -- NOT NULL enforces the rule: no source, no stored address.
    source_type  TEXT NOT NULL,
    source_url   TEXT NOT NULL,
    provenance   TEXT NOT NULL CHECK (provenance IN ('page', 'hunter_found')),
    verification contact_verification NOT NULL DEFAULT 'unknown',
    confidence   REAL,
    found_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (company_id, email)
);

CREATE TABLE drafts (
    id             BIGSERIAL PRIMARY KEY,
    company_id     BIGINT NOT NULL REFERENCES companies (id),
    contact_id     BIGINT NOT NULL REFERENCES contacts (id),
    subject        TEXT NOT NULL,
    body           TEXT NOT NULL,
    hook_signal_id BIGINT NOT NULL REFERENCES signals (id),
    status         draft_status NOT NULL DEFAULT 'pending',
    imap_uid       BIGINT,
    message_id     TEXT,
    pushed_at      TIMESTAMPTZ,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    sent_at        TIMESTAMPTZ,
    UNIQUE (company_id)
);

CREATE INDEX drafts_status_idx ON drafts (status);

CREATE TABLE seen_urls (
    url     TEXT PRIMARY KEY,
    seen_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE suppression (
    email      TEXT PRIMARY KEY,
    reason     TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE runs (
    id          BIGSERIAL PRIMARY KEY,
    job         TEXT NOT NULL,
    started_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ,
    stats_json  JSONB,
    error       TEXT
);
