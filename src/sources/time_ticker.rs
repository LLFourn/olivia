use crate::{
    core::{Event, EventId, EventOutcome, Outcome},
    db::Db,
    sources::Update,
};
use chrono::{Duration, NaiveDateTime};
use futures::{channel::oneshot, stream};
use std::sync::Arc;
use tokio::time;

pub fn time_events_stream(
    db: Arc<dyn Db>,
    look_ahead: Duration,
    interval: Duration,
    initial_time: NaiveDateTime,
    logger: slog::Logger,
) -> impl stream::Stream<Item = Update<Event>> {
    stream::unfold(None, move |waiting| {
        let db = db.clone();
        let logger = logger.clone();
        async move {
            if let Some(waiting) = waiting {
                let res: Result<_, _> = waiting.await;
                if res.is_err() {
                    // This should only happen when the last event we emitted cannot be stored in the DB for some reason.
                    // There is no way to recover from this and there is no reason to keep emitting events
                    crit!(logger, "Stopping emitting new time events as the consumer was unable to store one of our events");
                    return None;
                }
            }

            match db.latest_time_event().await {
                Ok(Some(latest)) => {
                    let latest = latest
                        .expected_outcome_time
                        .expect("time events always this");
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
                    return None;
                }
            }
        }
    })
}

pub fn time_outcomes_stream(
    db: Arc<dyn Db>,
    logger: slog::Logger,
) -> impl stream::Stream<Item = Update<EventOutcome>> {
    stream::unfold(None, move |waiting| {
        let db = db.clone();
        let logger = logger.clone();
        async move {
            if let Some(waiting) = waiting {
                let res: Result<_, _> = waiting.await;
                if res.is_err() {
                    error!(logger,"Stopping emitting time outcomes as the consumer was unable to store one of our events");
                    return None;
                }
            }
            let mut event;

            while {
                event = match db.clone().earliest_unattested_time_event().await {
                    Err(err) => {
                        crit!(
                            logger,
                            "Stopping emitting time outcomes as we got a DB error";
                            "error" => format!("{}", err)
                        );
                        return None;
                    }
                    Ok(event) => event,
                };

                event.is_none()
            } {
                time::delay_for(std::time::Duration::from_secs(1)).await
            }
            let event = event.unwrap();
            let event_complete_time = event
                .expected_outcome_time
                .expect("time events always have this");

            delay_until(event_complete_time).await;

            let (sender, receiver) = oneshot::channel();

            Some((
                Update {
                    update: EventOutcome {
                        event_id: event.id.clone(),
                        outcome: Outcome::Occurred,
                        time: now(), // tell the actual truth about when we actually figured it was done
                    },
                    processed_notifier: Some(sender),
                },
                (Some(receiver)),
            ))
        }
    })
}

/// coverts a time to an event update wrapped the way we need it to be for stream::unfold
fn time_to_event_update(
    dt: NaiveDateTime,
) -> Option<(Update<Event>, Option<oneshot::Receiver<()>>)> {
    let (sender, receiver) = oneshot::channel();
    Some((
        Update {
            update: Event::from(dt),
            processed_notifier: Some(sender),
        },
        Some(receiver),
    ))
}

impl From<NaiveDateTime> for Event {
    fn from(dt: NaiveDateTime) -> Self {
        let id = time_to_id(dt);
        Event {
            id,
            expected_outcome_time: Some(dt),
        }
    }
}

pub fn time_to_id(dt: NaiveDateTime) -> EventId {
    EventId(format!("time/{}.occur", dt.format("%FT%T")))
}

async fn delay_until(until: NaiveDateTime) {
    let delta = until - now();
    if delta > Duration::zero() {
        time::delay_for(delta.to_std().unwrap().into()).await;
    }
}

fn now() -> NaiveDateTime {
    chrono::Utc::now().naive_utc()
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::{core::AnnouncedEvent, db::in_memory::InMemory};
    use futures::stream::StreamExt;
    use std::str::FromStr;

    fn logger() -> slog::Logger {
        slog::Logger::root(slog::Discard, o!())
    }

    /// this is called from tests for particular DB to populate their
    /// db before called test_time_ticker_db
    pub fn time_ticker_db_test_data() -> Vec<AnnouncedEvent> {
        vec![
            {
                let time = NaiveDateTime::from_str("2020-03-01T00:25:00").unwrap();
                let mut obs_event = AnnouncedEvent::test_new(&time_to_id(time));
                obs_event.attestation = None;
                obs_event.event.expected_outcome_time = Some(time);
                obs_event
            },
            {
                let time = NaiveDateTime::from_str("2020-03-01T00:30:00").unwrap();
                let mut obs_event = AnnouncedEvent::test_new(&time_to_id(time));
                obs_event.attestation = None;
                obs_event.event.expected_outcome_time = Some(time);
                obs_event
            },
            {
                let time = NaiveDateTime::from_str("2020-03-01T00:20:00").unwrap();
                let mut obs_event = AnnouncedEvent::test_new(&time_to_id(time));
                obs_event.attestation = None;
                obs_event.event.expected_outcome_time = Some(time);
                obs_event
            },
            {
                // put in a non time event which *SHOULD* be ignored
                let time = NaiveDateTime::from_str("2020-03-01T00:11:00").unwrap();
                let mut obs_event =
                    AnnouncedEvent::test_new(&EventId::from_str("foo/bar/baz.occur").unwrap());
                obs_event.attestation = None;
                obs_event.event.expected_outcome_time = Some(time);
                obs_event
            },
            {
                let time = NaiveDateTime::from_str("2020-03-01T00:10:00").unwrap();
                let mut obs_event = AnnouncedEvent::test_new(&time_to_id(time));
                obs_event.event.expected_outcome_time = Some(time);
                obs_event.attestation.as_mut().unwrap().time = time;
                obs_event
            },
            {
                let time = NaiveDateTime::from_str("2020-03-01T00:05:00").unwrap();
                let mut obs_event = AnnouncedEvent::test_new(&time_to_id(time));
                obs_event.event.expected_outcome_time = Some(time);
                obs_event.attestation.as_mut().unwrap().time = time;
                obs_event
            },
            {
                let time = NaiveDateTime::from_str("2020-03-01T00:15:00").unwrap();
                let mut obs_event = AnnouncedEvent::test_new(&time_to_id(time));
                obs_event.event.expected_outcome_time = Some(time);
                obs_event.attestation.as_mut().unwrap().time = time;
                obs_event
            },
        ]
    }

    pub async fn test_time_ticker_db(db: Arc<dyn Db>) {
        let latest_time_event = db
            .latest_time_event()
            .await
            .expect("latest_time_event isn't Err")
            .expect("latest_time_event isn't None");

        assert_eq!(latest_time_event, time_ticker_db_test_data()[1].event);

        let earliest_unattested_time_event = db
            .earliest_unattested_time_event()
            .await
            .expect("earliest_unattested_time_event isn't Err")
            .expect("earliest_unattested_time_event isn't None");

        assert_eq!(
            earliest_unattested_time_event,
            time_ticker_db_test_data()[2].event
        );
    }

    #[test]
    fn time_ticker_events_stream() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        let db: Arc<dyn Db> = Arc::new(InMemory::default());
        let start = now();
        let look_ahead = Duration::seconds(2);
        let interval = Duration::seconds(1);
        let mut stream =
            time_events_stream(db.clone(), look_ahead, interval, start, logger()).boxed();
        let mut cur = start.clone();

        {
            let update = rt.block_on(stream.next()).expect("Not None");
            let event = update.update;
            assert_eq!(event.id, time_to_id(cur));
            rt.block_on(db.insert_event(AnnouncedEvent::from(event)))
                .unwrap();
            let _ = update.processed_notifier.unwrap().send(());
        }

        cur += interval;

        {
            let update = rt.block_on(stream.next()).expect("Not None");
            let event = update.update;
            assert_eq!(event.id, time_to_id(cur));
            rt.block_on(db.insert_event(AnnouncedEvent::from(event)))
                .unwrap();
            let _ = update.processed_notifier.unwrap().send(());
        }

        cur += interval;

        {
            let update = rt.block_on(stream.next()).expect("Not None");
            let event = update.update;
            assert_eq!(event.id, time_to_id(cur));
            rt.block_on(db.insert_event(AnnouncedEvent::from(event)))
                .unwrap();
            let _ = update.processed_notifier.unwrap().send(());
        }
        assert!(
            now() < start + Duration::milliseconds(100),
            "we shouldn't have waited for anything yet"
        );

        cur += interval;
        {
            let update = rt.block_on(stream.next()).expect("Not None");
            let event = update.update;
            assert_eq!(event.id, time_to_id(cur));
            rt.block_on(db.insert_event(AnnouncedEvent::from(event)))
                .unwrap();
            let _ = update.processed_notifier.unwrap().send(());
        }

        assert!(
            now() > start + Duration::seconds(1),
            "we should have waited for 1 second"
        );
        assert!(
            now() < start + Duration::milliseconds(1100),
            "shouldn't have waited too much"
        );
    }

    #[test]
    fn time_ticker_outcome_empty_db() {
        let db: Arc<dyn Db> = Arc::new(InMemory::default());
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let mut stream = time_outcomes_stream(db.clone(), logger()).boxed();
        let future = stream.next();
        assert!(
            rt.block_on(async move {
                tokio::time::timeout(std::time::Duration::from_millis(1), future).await
            })
            .is_err(),
            "Empty db should just block"
        );
    }

    #[test]
    fn time_ticker_outcome_in_future() {
        let db: Arc<dyn Db> = Arc::new(InMemory::default());
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        let start = now();

        rt.block_on(db.insert_event(AnnouncedEvent::from(Event::from(
            start + Duration::seconds(1),
        ))))
        .unwrap();
        let mut stream = time_outcomes_stream(db.clone(), logger()).boxed();
        let future = stream.next();

        assert!(
            rt.block_on(async move {
                tokio::time::timeout(std::time::Duration::from_millis(1), future).await
            })
            .is_err(),
            "db with event in the future should just block"
        );
    }

    #[test]
    fn time_ticker_outcome_with_event_in_past() {
        let db: Arc<dyn Db> = Arc::new(InMemory::default());
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        let start = now();

        rt.block_on(db.insert_event(AnnouncedEvent::from(Event::from(start))))
            .unwrap();

        let mut stream = time_outcomes_stream(db.clone(), logger()).boxed();
        let item = rt.block_on(stream.next()).expect("stream shouldn't stop");
        let outcome = item.update;
        assert!(
            now() < start + Duration::milliseconds(100),
            "should generate outcome for event in the past immediately"
        );

        assert_eq!(
            outcome.event_id,
            time_to_id(start),
            "outcome should be for the time that was inserted"
        );
        assert_eq!(
            outcome.outcome,
            Outcome::Occurred,
            "outcome string should be true"
        );
        assert!(
            outcome.time >= start,
            "the time of the outcome should be greater than when it was scheduled"
        );
        assert!(outcome.time <= now(), "should not be in the future");
    }

    #[test]
    fn time_ticker_wait_for_event_outcomes() {
        use crate::core::Attestation;
        let db: Arc<dyn Db> = Arc::new(InMemory::default());
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        let mut stream = time_outcomes_stream(db.clone(), logger()).boxed();
        let start = now();
        dbg!(&start);

        // add some time events in the future out of order
        rt.block_on(db.insert_event(AnnouncedEvent::from(Event::from(
            start + Duration::seconds(3),
        ))))
        .unwrap();

        rt.block_on(db.insert_event(AnnouncedEvent::from(Event::from(
            start + Duration::seconds(1),
        ))))
        .unwrap();

        rt.block_on(db.insert_event(AnnouncedEvent::from(Event::from(
            start + Duration::seconds(2),
        ))))
        .unwrap();

        // test that they get emitted in order
        let first = rt.block_on(stream.next()).unwrap();
        assert_eq!(
            first.update.event_id,
            time_to_id(start + Duration::seconds(1))
        );
        assert!(dbg!(now()) >= dbg!(start + Duration::seconds(1)));
        assert!(now() < start + Duration::milliseconds(1100));
        rt.block_on(db.complete_event(
            &first.update.event_id,
            Attestation::test_new(&first.update.event_id),
        ))
        .unwrap();
        first.processed_notifier.unwrap().send(()).unwrap();

        let second = rt.block_on(stream.next()).unwrap();
        assert_eq!(
            second.update.event_id,
            time_to_id(start + Duration::seconds(2))
        );
        assert!(dbg!(now()) >= dbg!(start + Duration::seconds(2)));
        assert!(now() < start + Duration::milliseconds(2100));
        rt.block_on(db.complete_event(
            &second.update.event_id,
            Attestation::test_new(&first.update.event_id),
        ))
        .unwrap();
        second.processed_notifier.unwrap().send(()).unwrap();

        let third = rt.block_on(stream.next()).unwrap();
        assert_eq!(
            third.update.event_id,
            time_to_id(start + Duration::seconds(3))
        );
        assert!(now() >= start + Duration::seconds(3));
        assert!(now() < start + Duration::milliseconds(3100));
    }
}
