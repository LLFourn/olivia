CREATE TABLE meta (
       key VARCHAR(255) NOT NULL PRIMARY KEY,
       value jsonb NOT NULL
);

CREATE table tree (
       id text NOT NULL PRIMARY KEY,
       parent text REFERENCES tree (id)
);

CREATE TABLE events (
       id text NOT NULL PRIMARY KEY,
       parent text NOT NULL REFERENCES tree (id),
       human_url text,
       kind jsonb NOT NULL,
       expected_outcome_time timestamp NOT NULL
);

CREATE TABLE nonces (
       event_id text NOT NULL PRIMARY KEY REFERENCES events (id),
       ed25519 bytea NOT NULL,
       secp256k1 bytea NOT NULL
);

CREATE TABLE attestations (
       event_id text NOT NULL PRIMARY KEY REFERENCES events (id),
       outcome text NOT NULL,
       time timestamp NOT NULL,
       ed25519 bytea NOT NULL,
       secp256k1 bytea NOT NULL
);


CREATE INDEX idx_expected_outcome_time ON events (expected_outcome_time DESC);
