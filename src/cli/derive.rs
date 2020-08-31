use crate::{config::Config, core::EventId, keychain::KeyChain};

pub fn derive(
    config: Config,
    event: EventId,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let secret_seed = config
        .secret_seed
        .ok_or("config file needs secret_seed to run".to_string())?;
    let keychain = KeyChain::new(secret_seed);
    let nonces = keychain.nonces_for_event(&event);

    println!("secp256k1: {:?}", nonces.secp256k1);
    println!("ed25519: {:?}", nonces.ed25519);
    Ok(())
}
