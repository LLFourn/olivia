use crate::{config::Config, core::Entity, Oracle, curve::CurveImpl};
use std::str::FromStr;

pub fn add(config: Config, entity: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let secret_seed = config
        .secret_seed
        .ok_or("Cannot use the add command when oracle is in read-only mode")?;
    let mut rt = tokio::runtime::Runtime::new()?;
    let db = config.database.connect_database::<CurveImpl>()?;
    let oracle = rt.block_on(Oracle::new(secret_seed, db.clone()))?;

    match Entity::from_str(entity)? {
        Entity::Event(event) => Ok(rt.block_on(oracle.add_event(event))?),
        Entity::Outcome(event_outcome) => Ok(rt.block_on(oracle.complete_event(event_outcome))?),
    }
}
