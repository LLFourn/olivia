use crate::{config::Config, keychain::KeyChain};
use olivia_core::EventId;

pub fn derive(config: Config, event: EventId) -> anyhow::Result<()> {
    let secret_seed = config
        .secret_seed
        .ok_or(anyhow::anyhow!("config file needs secret_seed to run"))?;
    let keychain = KeyChain::<olivia_secp256k1::Secp256k1>::new(secret_seed);
    let nonce = keychain.nonces_for_event(&event);

    println!("secp256k1: {:?}", nonce);
    Ok(())
}
