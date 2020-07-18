table! {
    announcements (event_id) {
        event_id -> Text,
        ed25519_nonce -> Bytea,
        ed25519_signature -> Bytea,
        secp256k1_nonce -> Bytea,
        secp256k1_signature -> Bytea,
    }
}

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
        node -> Text,
        expected_outcome_time -> Nullable<Timestamp>,
    }
}

table! {
    meta (key) {
        key -> Varchar,
        value -> Jsonb,
    }
}

table! {
    tree (id) {
        id -> Text,
        parent -> Nullable<Text>,
    }
}

joinable!(announcements -> events (event_id));
joinable!(attestations -> events (event_id));
joinable!(events -> tree (node));

allow_tables_to_appear_in_same_query!(
    announcements,
    attestations,
    events,
    meta,
    tree,
);
