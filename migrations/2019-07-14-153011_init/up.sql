CREATE TABLE meta (
       key VARCHAR(256) NOT NULL PRIMARY KEY,
       value jsonb NOT NULL
);

CREATE TABLE events (
       id VARCHAR(256) NOT NULL PRIMARY KEY,
       path text[] NOT NULL,
       human_url VARCHAR(2048),
       kind jsonb NOT NULL,
       expected_outcome_time timestamp NOT NULL
);

CREATE TABLE nonces (
       event_id VARCHAR(256) NOT NULL PRIMARY KEY REFERENCES events (id),
       ed25519 bytea NOT NULL,
       secp256k1 bytea NOT NULL
);

CREATE TABLE attestations (
       event_id VARCHAR(256) NOT NULL PRIMARY KEY REFERENCES events (id),
       outcome VARCHAR NOT NULL,
       time timestamp NOT NULL,
       ed25519 bytea NOT NULL,
       secp256k1 bytea NOT NULL
);


CREATE INDEX idx_path ON events (path);
CREATE INDEX idx_expected_outcome_time ON events (expected_outcome_time DESC);
