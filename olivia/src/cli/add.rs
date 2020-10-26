use olivia_core::StampedOutcome;

use crate::{config::Config, core::Entity, curve::SchnorrImpl, Oracle};
use std::str::FromStr;

pub fn add(config: Config, entity: &str) -> anyhow::Result<()> {
    let secret_seed = config.secret_seed.ok_or(anyhow::anyhow!(
        "Cannot use the add command when oracle is in read-only mode"
    ))?;
    let mut rt = tokio::runtime::Runtime::new()?;
    let db = config.database.connect_database::<SchnorrImpl>()?;
    let oracle = rt.block_on(Oracle::new(secret_seed, db.clone()))?;

    match Entity::from_str(entity)? {
        Entity::Event(event) => Ok(rt.block_on(oracle.add_event(event))?),
        Entity::Outcome(outcome) => {
            let event_outcome = StampedOutcome {
                time: chrono::Utc::now().naive_utc(),
                outcome,
            };
            rt.block_on(oracle.complete_event(event_outcome))?;
            Ok(())
        }
    }
}
