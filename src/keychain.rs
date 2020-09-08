use crate::{
    core::{Announcement, EventId, EventOutcome, Schnorr},
    curve::DeriveKeyPair,
    seed::Seed,
};

pub struct KeyChain<C: Schnorr + DeriveKeyPair> {
    keypair: C::KeyPair,
    event_seed: Seed,
}

impl<C: Schnorr + DeriveKeyPair> KeyChain<C> {
    pub fn new(seed: Seed) -> Self {
        let key_seed = seed.child(b"oracle-key");
        let keypair = C::derive_keypair(&key_seed);
        Self {
            keypair,
            event_seed: seed.child(b"oracle-events"),
        }
    }

    pub fn oracle_public_key(&self) -> C::PublicKey {
        self.keypair.clone().into()
    }

    pub fn nonce_for_event(&self, event_id: &EventId) -> C::NonceKeyPair {
        let event_idx = self.event_seed.child(event_id.as_bytes());
        C::derive_nonce_keypair(&event_idx)
    }

    pub fn scalar_for_event_outcome(&self, outcome: &EventOutcome) -> C::SigScalar {
        let outcome_long_id = outcome.attestation_string();
        let event_idx = self.event_seed.child(outcome.event_id.as_bytes());

        C::reveal_signature_s(
            &self.keypair,
            C::derive_nonce_keypair(&event_idx),
            outcome_long_id.as_bytes(),
        )
    }

    pub fn create_announcement(&self, event_id: &EventId) -> Announcement<C> {
        let nonce = self.nonce_for_event(&event_id).into();
        Announcement::create(event_id, &self.keypair, nonce)
    }
}
