CREATE EXTENSION ltree;

CREATE TYPE announcement AS (
       oracle_event bytea,
       signature bytea
       );

CREATE TYPE attestation AS (
       outcome text,
       time timestamp,
       olivia_v1_scalars bytea[],
       ecdsa_v1_signature bytea
);

CREATE TABLE meta (
       key VARCHAR(255) NOT NULL PRIMARY KEY,
       value jsonb NOT NULL
);

CREATE table tree (
       id text NOT NULL PRIMARY KEY,
       parent text REFERENCES tree (id),
       kind jsonb
);

CREATE TABLE event (
       id text NOT NULL PRIMARY KEY,
       expected_outcome_time timestamp,
       ann announcement,
       att attestation,
       path ltree
       CONSTRAINT attest_valid
       CHECK ((att).outcome IS NULL OR (att).time IS NOT NULL)
);

CREATE INDEX idx_expected_outcome_time ON event (expected_outcome_time DESC);
-- To lookup all children of a node e.g. to find all games under /NBA/2021-10-04
CREATE INDEX idx_lookup_node_by_parent ON tree USING HASH (parent);
-- So we can do SELECT min(id), max(id) FROM tree WHERE parent = '/time' efficiently
CREATE INDEX min_max_node_id ON tree (parent, id);
-- TODO: We need an index which makes looking up the earliest unattested event faster
CREATE INDEX idx_path_gist ON event USING GIST (path);

INSERT INTO meta (key, value) VALUES ('version', '{"version" : 0 }'::jsonb);
