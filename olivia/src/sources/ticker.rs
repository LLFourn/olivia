use crate::{
    db::{DbReadEvent, PrefixedDb},
    sources::Update,
};
use olivia_core::{
    chrono,
    chrono::{Duration, NaiveDateTime},
    Event, EventId, Outcome, PathRef, StampedOutcome,
};
use tokio::{sync::oneshot, time};
use tokio_stream as stream;

pub struct TimeEventStream<F> {
    pub db: PrefixedDb,
    pub look_ahead: Duration,
    pub interval: Duration,
    pub initial_time: NaiveDateTime,
    pub logger: slog::Logger,
    pub event_creator: F,
}

impl<F> TimeEventStream<F>
where
    F: 'static + Sync + Send + Fn(NaiveDateTime) -> EventId,
{
    pub fn start(self) -> impl stream::Stream<Item = Update<Event>> {
        let TimeEventStream {
            db,
            look_ahead,
            interval,
            initial_time,
            logger,
            event_creator,
        } = self;

        let create_update = move |dt| {
            let (sender, receiver) = oneshot::channel();
            (
                Update {
                    update: Event {
                        id: event_creator(dt),
                        expected_outcome_time: Some(dt),
                    },
                    processed_notifier: Some(sender),
                },
                receiver,
            )
        };

        async_stream::stream! {
            loop  {
                let latest = db.latest_child_event(PathRef::root()).await;
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
    pub outcome_creator: F,
}

impl<F> TimeOutcomeStream<F>
where
    F: 'static + Send + Sync + Fn(EventId) -> Outcome,
{
    pub fn start(self) -> impl stream::Stream<Item = Update<StampedOutcome>> {
        let TimeOutcomeStream {
            db,
            logger,
            outcome_creator,
        } = self;
        async_stream::stream! {
            loop {
                let event = db.earliest_unattested_child_event(PathRef::root()).await;
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
                        outcome: outcome_creator(event.id.clone()),
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
