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
       node text NOT NULL REFERENCES tree (id),
       expected_outcome_time timestamp
);

CREATE TABLE announcements (
       event_id text NOT NULL PRIMARY KEY REFERENCES events (id),
       oracle_event bytea NOT NULL,
       signature bytea NOT NULL
);

CREATE TABLE attestations (
       event_id text NOT NULL PRIMARY KEY REFERENCES events (id),
       outcome text NOT NULL,
       time timestamp NOT NULL,
       scalars bytea[] NOT NULL
);

CREATE INDEX idx_expected_outcome_time ON events (expected_outcome_time DESC);
