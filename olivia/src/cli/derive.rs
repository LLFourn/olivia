use crate::{config::Config, core::EventId, curve::SchnorrImpl, keychain::KeyChain};

pub fn derive(
    config: Config,
    event: EventId,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let secret_seed = config
        .secret_seed
        .ok_or("config file needs secret_seed to run".to_string())?;
    let keychain = KeyChain::<SchnorrImpl>::new(secret_seed);
    let nonce = keychain.nonce_for_event(&event);

    println!("secp256k1: {:?}", nonce);
    Ok(())
}
