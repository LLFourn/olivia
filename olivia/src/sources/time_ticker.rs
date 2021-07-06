use crate::{
    db::{DbReadEvent, PrefixedDb},
    sources::Update,
};
use chrono::{Duration, NaiveDateTime};
use futures::{channel::oneshot, stream};
use olivia_core::{Event, EventKind, Outcome, PathRef, StampedOutcome};
use tokio::time;

pub struct TimeEventStream {
    pub db: PrefixedDb,
    pub look_ahead: Duration,
    pub interval: Duration,
    pub initial_time: NaiveDateTime,
    pub logger: slog::Logger,
}

impl TimeEventStream {
    pub fn start(self) -> impl stream::Stream<Item = Update<Event>> {
        let TimeEventStream {
            db,
            look_ahead,
            interval,
            initial_time,
            logger,
        } = self;
        let (sender, receiver) = futures::channel::mpsc::unbounded();

        tokio::spawn(async move {
            loop {
                let latest = db
                    .latest_child_event(PathRef::root(), EventKind::SingleOccurrence)
                    .await;
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
                        time_to_event_update(next_event)
                    }
                    Ok(None) => {
                        // This means this is our first run against this backend, we add a new event to get us started.
                        time_to_event_update(initial_time)
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

                if let Err(_) = sender.unbounded_send(update) {
                    break;
                }

                if let Err(_) | Ok(true) = waiting.await {
                    error!(logger, "processing of new time event failed (will try again)"; "id" => event_id.as_str());
                    time::sleep(std::time::Duration::from_secs(10)).await;
                }
            }
            ();
        });

        receiver
    }
}

pub struct TimeOutcomeStream {
    pub db: PrefixedDb,
    pub logger: slog::Logger,
}

impl TimeOutcomeStream {
    pub fn start(self) -> impl stream::Stream<Item = Update<StampedOutcome>> {
        let TimeOutcomeStream { db, logger } = self;
        let (stream_sender, stream_receiver) = futures::channel::mpsc::unbounded();
        tokio::spawn(async move {
            loop {
                let event = db
                    .earliest_unattested_child_event(PathRef::root(), EventKind::SingleOccurrence)
                    .await;
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

                let update = Update {
                    update: StampedOutcome {
                        outcome: Outcome {
                            id: event.id.clone(),
                            value: 0,
                        },
                        time: now(), // tell the actual truth about when we actually figured it was done
                    },
                    processed_notifier: Some(sender),
                };

                if let Err(_) = stream_sender.unbounded_send(update) {
                    crit!(
                        logger,
                        "receiver died for outcomes -- shutting down emitter"
                    );
                    break;
                }

                if let Err(_) | Ok(true) = waiting.await {
                    error!(logger, "processing of outcome for failed (will try again)"; "id" => event.id.as_str());
                    time::sleep(std::time::Duration::from_secs(10)).await;
                }
            }
        });

        stream_receiver
    }
}

fn time_to_event_update(dt: NaiveDateTime) -> (Update<Event>, oneshot::Receiver<bool>) {
    let (sender, receiver) = oneshot::channel();
    (
        Update {
            update: Event::occur_event_from_dt(dt),
            processed_notifier: Some(sender),
        },
        receiver,
    )
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
