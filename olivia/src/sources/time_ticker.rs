use crate::{db::BorrowDb, sources::Update};
use chrono::{Duration, NaiveDateTime};
use futures::{channel::oneshot, stream};
use olivia_core::{Event, EventKind, Group, Outcome, StampedOutcome};
use tokio::time;

pub fn time_events_stream<C: Group, D: BorrowDb<C>>(
    db: D,
    look_ahead: Duration,
    interval: Duration,
    initial_time: NaiveDateTime,
    logger: slog::Logger,
) -> impl stream::Stream<Item = Update<Event>> {
    let (sender, receiver) = futures::channel::mpsc::unbounded();

    tokio::spawn(async move {
        loop {
            let latest = db
                .borrow_db()
                .latest_child_event("/time", EventKind::SingleOccurrence)
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
                        "error" => format!("{}",err),
                    );
                    break;
                }
            };

            if let Err(_) = sender.unbounded_send(update) {
                break;
            }

            let res: Result<_, _> = waiting.await;
            if res.is_err() {
                // This should only happen when the last event we emitted cannot be stored in the DB for some reason.
                // There is no way to recover from this and there is no reason to keep emitting events
                crit!(logger, "Stopping emitting new time events as the consumer was unable to store one of our events");
                break;
            }
        }
        ();
    });

    receiver
}

pub fn time_outcomes_stream<C: Group, D: BorrowDb<C>>(
    db: D,
    logger: slog::Logger,
) -> impl stream::Stream<Item = Update<StampedOutcome>> {
    let (stream_sender, stream_receiver) = futures::channel::mpsc::unbounded();
    tokio::spawn(async move {
        loop {
            let event = db
                .borrow_db()
                .earliest_unattested_child_event("/time", EventKind::SingleOccurrence)
                .await;
            let event = match event {
                Ok(Some(event)) => event,
                Err(e) => {
                    crit!(
                        logger,
                        "DB error during /time outcome stream";
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
                    "receiver died for /time outcomes -- shutting down emitter"
                );
                break;
            }

            if let Err(_) = waiting.await {
                error!(logger, "processing of outcome for failed (will try again)"; "id" => event.id.as_str());
                time::sleep(std::time::Duration::from_secs(10)).await;
            }
        }
    });

    stream_receiver
}

/// coverts a time to an event update wrapped the way we need it to be for stream::unfold
fn time_to_event_update(dt: NaiveDateTime) -> (Update<Event>, oneshot::Receiver<()>) {
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

// #[cfg(test)]
// pub mod test {
//     use super::*;
//     use crate::{
//         core::{AnnouncedEvent, EventId, Group},
//         db::Db,
//     };
//     use futures::{Future, stream::StreamExt};
//     use std::{str::FromStr, sync::Arc};

//     pub async fn run_time_db_tests<C: Group, D: Db<C>, F: Future<Output=D>>(mut gen: impl FnMut() -> F) {
//         // test_time_range_db(&gen().await).await;
//         // time_ticker_events_stream(Arc::new(gen().await)).await;
//         // time_ticker_outcome_empty_db(gen().await).await;
//         // time_ticker_outcome_in_future(gen().await).await;
//         // time_ticker_outcome_with_event_in_past(gen().await).await;
//         time_ticker_wait_for_event_outcomes(Arc::new(gen().await)).await;
//     }

// }
