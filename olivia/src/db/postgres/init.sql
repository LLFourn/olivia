CREATE TYPE announcement AS (
       oracle_event bytea,
       signature bytea
       );

CREATE TYPE attestation AS (
       outcome text,
       time timestamp,
       scalars bytea[]
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
       node text NOT NULL REFERENCES tree (id),
       expected_outcome_time timestamp,
       ann announcement,
       att attestation
       CONSTRAINT attest_valid
       CHECK ( ((att).outcome IS NULL) OR ((att).time IS NOT NULL AND (att).scalars IS NOT NULL))
);

CREATE INDEX idx_expected_outcome_time ON event (expected_outcome_time DESC);

-- To lookup an event by node e.g. all events connected to /time/202-06-24T10:00:00 like /time/202-06-24T10:00:00.occur
CREATE INDEX idx_lookup_event_by_node ON event USING HASH (node);
-- To lookup all children of a node e.g. to find all games under /NBA/2021-10-04
CREATE INDEX idx_lookup_node_by_parent ON tree USING HASH (parent);
-- So we can do SELECT min(id), max(id) FROM tree WHERE parent = '/time' efficiently
CREATE INDEX min_max_node_id ON tree (parent, id);
-- TODO: We need an index which makes looking up the earliest unattested event faster
