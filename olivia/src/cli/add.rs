use crate::{config::Config, Oracle};
use olivia_core::{StampedOutcome, chrono::{self, NaiveDateTime}, Outcome, EventId, Event};

#[derive(Debug, structopt::StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub enum Entity {
    Event { event_id: EventId, expected_outcome_time: Option<NaiveDateTime> },
    Outcome { event_id: EventId, outcome: String }
}

pub async fn add(config: Config, entity: Entity) -> anyhow::Result<()> {
    let secret_seed = config.secret_seed.ok_or(anyhow::anyhow!(
        "Cannot use the add command when oracle is in read-only mode"
    ))?;
    let db = config.database.connect_database().await?;
    let oracle = Oracle::new(secret_seed, db.clone()).await?;

    match entity {
        Entity::Event { event_id, expected_outcome_time } => oracle.add_event(Event { id: event_id, expected_outcome_time }).await?,
        Entity::Outcome { event_id, outcome } => {
            let outcome = Outcome::try_from_id_and_outcome(event_id, &outcome)?;
            oracle.complete_event(StampedOutcome { time: chrono::Utc::now().naive_utc(), outcome }).await?;
        }
    }

    Ok(())
}
