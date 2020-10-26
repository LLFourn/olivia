use crate::{
    core::{Event, EventId, RawAnnouncement, Schnorr, StampedOutcome},
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

    pub fn nonces_for_event(&self, event_id: &EventId) -> Vec<C::NonceKeyPair> {
        let event_idx = self.event_seed.child(event_id.as_bytes());
        let n = event_id.event_kind().n_fragments();
        (0..n)
            .map(|i| C::derive_nonce_keypair(&event_idx, i as u32))
            .collect()
    }

    pub fn scalars_for_event_outcome(&self, stamped: &StampedOutcome) -> Vec<C::SigScalar> {
        let event_id = &stamped.outcome.id;
        let event_idx = self.event_seed.child(event_id.as_bytes());
        stamped
            .outcome
            .fragments()
            .into_iter()
            .map(|fragment| {
                C::reveal_signature_s(
                    &self.keypair,
                    C::derive_nonce_keypair(&event_idx, fragment.index as u32),
                    fragment.attestation_string().as_bytes(),
                )
            })
            .collect()
    }

    pub fn create_announcement(&self, event: Event) -> RawAnnouncement<C> {
        let nonces = self
            .nonces_for_event(&event.id)
            .into_iter()
            .map(|nonce_kp| nonce_kp.into())
            .collect::<Vec<_>>();
        RawAnnouncement::create(event, &self.keypair, nonces)
    }
}
