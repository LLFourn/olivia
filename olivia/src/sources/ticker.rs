use crate::{
    db::{DbReadEvent, EventQuery, Order, PrefixedDb},
    seed::Seed,
    sources::Update,
};
use olivia_core::{
    chrono,
    chrono::{Duration, NaiveDateTime},
    Event, EventId, EventKind, Outcome, Path, PrefixPath, StampedOutcome,
};
use tokio::{sync::oneshot, time};
use tokio_stream as stream;

pub struct TimeEventStream {
    pub db: PrefixedDb,
    pub look_ahead: Duration,
    pub interval: Duration,
    pub initial_time: NaiveDateTime,
    pub logger: slog::Logger,
    pub ends_with: Path,
    pub event_kind: EventKind,
}

impl TimeEventStream {
    pub fn start(self) -> impl stream::Stream<Item = Update<Event>> {
        let TimeEventStream {
            db,
            look_ahead,
            interval,
            initial_time,
            logger,
            ends_with,
            event_kind,
        } = self;

        async_stream::stream! {
            let create_update = |dt| {
                let id = EventId::from_path_and_kind(ends_with.clone().prefix_path(Path::from_dt(dt).as_path_ref()), event_kind.clone());
                let (sender, receiver) = oneshot::channel();
                (
                    Update {
                        update: Event {
                            id,
                            expected_outcome_time: Some(dt),
                        },
                        processed_notifier: Some(sender),
                    },
                    receiver,
                )
            };

            loop  {
                let latest = db.query_event(EventQuery {
                    ends_with: ends_with.as_path_ref(),
                    kind: Some(event_kind.clone()),
                    order: Order::Latest,
                    ..Default::default()
                }).await;
                let (update, waiting) = match latest {
                    Ok(Some(latest)) => {
                        let latest = latest
                            .expected_outcome_time
                            .expect("time events always have this");
                        // If the latest event we have in the DB is 19:36 and our interval is 1min
                        // then the next event we want is 19:37.
                        let next_event = latest + interval;
                        // But we should add it at 18:36 if our look_ahead is 1hr
                        let add_when = next_event - look_ahead;
                        // wait until then before returning it
                        delay_until(add_when).await;
                        create_update(next_event)
                    }
                    Ok(None) => {
                        // This means this is our first run against this backend, we add a new event to get us started.
                        create_update(initial_time)
                    }
                    Err(err) => {
                        crit!(
                            logger,
                            "Stopping emitting new time events as we got a DB error";
                            "error" => err.to_string()
                        );
                        break;
                    }
                };
                let event_id = update.update.id.clone();

                yield update;

                if let Err(_) | Ok(true) = waiting.await {
                    error!(logger, "processing of new ticker failed (will try again)"; "id" => event_id.as_str());
                    time::sleep(std::time::Duration::from_secs(10)).await;
                }
            }
        }
    }
}

pub struct TimeOutcomeStream<F> {
    pub db: PrefixedDb,
    pub logger: slog::Logger,
    pub ends_with: Path,
    pub event_kind: Option<EventKind>,
    pub outcome_creator: F,
}

impl<F> TimeOutcomeStream<F>
where
    F: OutcomeCreator,
{
    pub fn start(self) -> impl stream::Stream<Item = Update<StampedOutcome>> {
        let TimeOutcomeStream {
            db,
            logger,
            outcome_creator,
            ends_with,
            event_kind,
        } = self;
        async_stream::stream! {
            loop {
                let event = db.query_event(EventQuery {
                    attested: Some(false),
                    order: Order::Earliest,
                    ends_with: ends_with.as_path_ref(),
                    kind: event_kind.clone(),
                    ..Default::default()
                }).await;
                let event = match event {
                    Ok(Some(event)) => event,
                    Err(e) => {
                        crit!(
                            logger,
                            "DB error during outcome stream";
                            "error" => format!("{}", e)
                        );
                        time::sleep(std::time::Duration::from_secs(60)).await;
                        continue;
                    }
                    Ok(None) => {
                        time::sleep(std::time::Duration::from_secs(1)).await;
                        continue;
                    }
                };

                let event_complete_time = event
                    .expected_outcome_time
                    .expect("time events always have this");

                delay_until(event_complete_time).await;

                let (sender, waiting) = oneshot::channel();

                yield Update {
                    update: StampedOutcome {
                        outcome: Outcome {
                            id: event.id.clone(),
                            value: outcome_creator.create_outcome(&event.id),
                        },
                        time: now(), // tell the actual truth about when we actually figured it was done
                    },
                    processed_notifier: Some(sender),
                };

                if let Err(_) | Ok(true) = waiting.await {
                    error!(logger, "processing of ticker outcome failed (will try again)"; "id" => event.id.as_str());
                    time::sleep(std::time::Duration::from_secs(10)).await;
                }
            }
        }
    }
}

async fn delay_until(until: NaiveDateTime) {
    let delta = until - now();
    if delta > Duration::zero() {
        time::sleep(delta.to_std().unwrap().into()).await;
    }
}

fn now() -> NaiveDateTime {
    chrono::Utc::now().naive_utc()
}

pub trait OutcomeCreator {
    fn create_outcome(&self, id: &EventId) -> u64;
}

pub struct RandomOutcomeCreator {
    pub seed: Seed,
    pub max: Option<u64>,
}

impl OutcomeCreator for RandomOutcomeCreator {
    fn create_outcome(&self, id: &EventId) -> u64 {
        use rand::{Rng, SeedableRng};
        let event_randomness = self.seed.child(id.as_bytes());
        let mut chacha_bytes = [0u8; 32];
        chacha_bytes.copy_from_slice(&event_randomness.as_ref()[..32]);
        let mut rng = chacha20::ChaCha20Rng::from_seed(chacha_bytes);
        let n_outcomes = id.n_outcomes();
        let max = self.max.unwrap_or(n_outcomes).min(n_outcomes);
        rng.gen_range(0..max)
    }
}

pub struct ZeroOutcomeCreator;

impl OutcomeCreator for ZeroOutcomeCreator {
    fn create_outcome(&self, _: &EventId) -> u64 {
        0
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn random_outcome_creator() {
        let random_outcome_creator = RandomOutcomeCreator {
            seed: Seed::new([42u8; 64]),
            max: None,
        };
        let random_outcomes = (0..10)
            .map(|i| {
                random_outcome_creator
                    .create_outcome(&EventId::from_str(&format!("/{}/foo_bar.vs", i)).unwrap())
            })
            .collect::<Vec<_>>();
        assert_eq!(random_outcomes, [0, 2, 2, 1, 2, 0, 1, 2, 0, 0].to_vec())
    }
}
