use crate::{config::Config, core::EventId, curve::SchnorrImpl, keychain::KeyChain};

pub fn derive(config: Config, event: EventId) -> anyhow::Result<()> {
    let secret_seed = config
        .secret_seed
        .ok_or(anyhow::anyhow!("config file needs secret_seed to run"))?;
    let keychain = KeyChain::<SchnorrImpl>::new(secret_seed);
    let nonce = keychain.nonces_for_event(&event);

    println!("secp256k1: {:?}", nonce);
    Ok(())
}
