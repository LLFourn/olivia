table! {
    attestations (event_id) {
        event_id -> Text,
        outcome -> Text,
        time -> Timestamp,
        ed25519 -> Bytea,
        secp256k1 -> Bytea,
    }
}

table! {
    events (id) {
        id -> Text,
        parent -> Text,
        human_url -> Nullable<Text>,
        kind -> Jsonb,
        expected_outcome_time -> Timestamp,
    }
}

table! {
    meta (key) {
        key -> Varchar,
        value -> Jsonb,
    }
}

table! {
    nonces (event_id) {
        event_id -> Text,
        ed25519 -> Bytea,
        secp256k1 -> Bytea,
    }
}

table! {
    tree (id) {
        id -> Text,
        parent -> Text,
    }
}

joinable!(attestations -> events (event_id));
joinable!(events -> tree (parent));
joinable!(nonces -> events (event_id));

allow_tables_to_appear_in_same_query!(
    attestations,
    events,
    meta,
    nonces,
    tree,
);
