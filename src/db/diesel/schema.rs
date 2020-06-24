table! {
    attestations (event_id) {
        event_id -> Varchar,
        outcome -> Varchar,
        time -> Timestamp,
        ed25519 -> Bytea,
        secp256k1 -> Bytea,
    }
}

table! {
    events (id) {
        id -> Varchar,
        path -> Array<Text>,
        human_url -> Nullable<Varchar>,
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
        event_id -> Varchar,
        ed25519 -> Bytea,
        secp256k1 -> Bytea,
    }
}

joinable!(attestations -> events (event_id));
joinable!(nonces -> events (event_id));

allow_tables_to_appear_in_same_query!(
    attestations,
    events,
    meta,
    nonces,
);
