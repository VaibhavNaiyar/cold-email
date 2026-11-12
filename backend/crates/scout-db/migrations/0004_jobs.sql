ALTER TABLE companies ADD COLUMN careers_checked_at TIMESTAMPTZ;

-- A careers board the job finder reads every run.
CREATE TABLE job_boards (
    id              BIGSERIAL PRIMARY KEY,
    company_id      BIGINT REFERENCES companies (id),
    name            TEXT NOT NULL,
    ats             TEXT NOT NULL CHECK (ats IN ('greenhouse', 'lever', 'ashby', 'yc')),
    slug            TEXT NOT NULL,
    source          TEXT NOT NULL CHECK (source IN ('yc', 'website', 'manual')),
    last_fetched_at TIMESTAMPTZ,
    last_error      TEXT,
    UNIQUE (ats, slug)
);

-- Only roles that passed the profile's title and place filter are stored.
CREATE TABLE jobs (
    id            BIGSERIAL PRIMARY KEY,
    board_id      BIGINT NOT NULL REFERENCES job_boards (id),
    external_id   TEXT NOT NULL,
    title         TEXT NOT NULL,
    location      TEXT NOT NULL,
    remote        BOOLEAN NOT NULL,
    department    TEXT,
    url           TEXT NOT NULL,
    posted_at     TIMESTAMPTZ,
    description   TEXT,
    notes         TEXT,
    first_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    closed_at     TIMESTAMPTZ,
    fit           INTEGER,
    fit_reason    TEXT,
    UNIQUE (board_id, external_id)
);

CREATE INDEX jobs_open_idx ON jobs (board_id) WHERE closed_at IS NULL;
