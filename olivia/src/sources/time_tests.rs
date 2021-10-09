#[macro_export]
#[doc(hidden)]
macro_rules! run_time_db_tests {
    (db => $db:ident,
     event_db => $event_db:ident,
     curve => $curve:ty, { $($init:tt)* }) => {

        #[allow(redundant_semicolons, unused_imports, unused_variables)]
        mod time_db_test {
            use super::*;
            use olivia_core::{AnnouncedEvent, EventKind, Event, EventId, path, PrefixPath, Path, Outcome, chrono::{NaiveDateTime, Duration, Utc}};
            use crate::db::Order;
            use crate::sources::ticker::*;
            use core::str::FromStr;
            use tokio_stream::StreamExt;
            use std::sync::Arc;

            fn logger() -> slog::Logger {
                slog::Logger::root(slog::Discard, o!())
            }

            fn now() -> NaiveDateTime {
                Utc::now().naive_utc()
            }

            #[tokio::test]
            async fn time_ticker_events_stream() {
                $($init)*;
                let look_ahead = Duration::seconds(2);
                let interval = Duration::seconds(1);
                let fudge = Duration::milliseconds(400);
                let initial_time = now();

                let mut stream = Box::pin(TimeEventStream {
                    db: PrefixedDb::new($event_db, Path::from_str("/time").unwrap()),
                    look_ahead,
                    interval,
                    initial_time,
                    logger: logger(),
                    ends_with: Path::root(),
                    event_kind: EventKind::SingleOccurrence,
                }.start());
                let mut cur = initial_time.clone();

                {
                    let update = stream.next().await.expect("Not None");
                    let event = update.update;
                    assert_eq!(event.id, EventId::occur_from_dt(cur), "one");
                    $db.insert_event(AnnouncedEvent::test_unattested_instance(event.prefix_path(path!("/time"))))
                       .await
                       .unwrap();
                    let _ = update.processed_notifier.unwrap().send(false);
                }

                cur += interval;

                {
                    let update = stream.next().await.expect("Not None");
                    let event = update.update;
                    assert_eq!(event.id, EventId::occur_from_dt(cur), "two");
                    $db.insert_event(AnnouncedEvent::test_unattested_instance(event.prefix_path(path!("/time"))))
                       .await
                       .unwrap();
                    let _ = update.processed_notifier.unwrap().send(false);
                }

                cur += interval;

                {
                    let update = stream.next().await.expect("Not None");
                    let event = update.update;
                    assert_eq!(event.id, EventId::occur_from_dt(cur));
                    $db.insert_event(AnnouncedEvent::test_unattested_instance(event.prefix_path(path!("/time"))))
                       .await
                       .unwrap();
                    let _ = update.processed_notifier.unwrap().send(false);
                }
                assert!(
                    now() < initial_time + fudge,
                    "we shouldn't have waited for anything yet"
                );

                cur += interval;
                {
                    let update = stream.next().await.expect("Not None");
                    let event = update.update;
                    assert_eq!(event.id, EventId::occur_from_dt(cur));
                    $db.insert_event(AnnouncedEvent::test_unattested_instance(event.prefix_path(path!("/time"))))
                       .await
                       .unwrap();
                    let _ = update.processed_notifier.unwrap().send(false);
                }

                assert!(
                    now() > initial_time + Duration::seconds(1),
                    "we should have waited for 1 second"
                );
                assert!(
                    now() < initial_time + Duration::seconds(1) + fudge,
                    "shouldn't have waited too much"
                );
            }

            fn time_outcome_stream(db: Arc<dyn DbReadEvent>) -> std::pin::Pin<Box<dyn tokio_stream::Stream<Item = crate::sources::Update<olivia_core::StampedOutcome>>>> {
                Box::pin(TimeOutcomeStream { outcome_creator: ZeroOutcomeCreator, db: PrefixedDb::new(db, Path::from_str("/time").unwrap()), logger: logger(), ends_with: Path::root(), event_kind: Some(EventKind::SingleOccurrence) }.start())
            }

            #[tokio::test]
            async fn time_ticker_outcome_empty_db() {
                $($init)*;
                let mut stream = time_outcome_stream($event_db);
                let future = stream.next();
                assert!(
                    tokio::time::timeout(std::time::Duration::from_millis(1), future)
                        .await
                        .is_err(),
                    "Empty db should just block"
                );
            }

            #[tokio::test]
            async fn time_ticker_outcome_in_future() {
                $($init)*;
                let start = now();

                $db.insert_event(AnnouncedEvent::test_unattested_instance(Event::occur_event_from_dt(
                    start + Duration::seconds(1),
                )))
                   .await
                   .unwrap();
                let mut stream = time_outcome_stream($event_db);
                let future = stream.next();

                assert!(
                    tokio::time::timeout(std::time::Duration::from_millis(1), future)
                        .await
                        .is_err(),
                    "db with event in the future should just block"
                );
            }

            #[tokio::test]
            async fn time_ticker_outcome_with_event_in_past() {
                $($init)*;
                let start = now();

                $db.insert_event(AnnouncedEvent::test_unattested_instance(Event::occur_event_from_dt(
                    start,
                ).prefix_path(path!("/time"))))
                   .await
                   .unwrap();


                let mut stream = time_outcome_stream($event_db);
                let item = stream.next().await.expect("stream shouldn't stop");
                let stamped = item.update;
                assert!(
                    now() < start + Duration::milliseconds(100),
                    "should generate outcome for event in the past immediately"
                );

                assert_eq!(
                    stamped.outcome.id,
                    EventId::occur_from_dt(start),
                    "outcome should be for the time that was inserted"
                );
                assert_eq!(
                    stamped.outcome.value,
                    olivia_core::Occur::Occurred as u64,
                    "outcome string should be true"
                );
                assert!(
                    stamped.time >= start,
                    "the time of the outcome should be greater than when it was scheduled"
                );
                assert!(stamped.time <= now(), "should not be in the future");
            }

            #[tokio::test]
            async fn time_ticker_wait_for_event_outcomes() {
                $($init)*;
                let start = now();
                let fudge = 900;

                let events = vec![
                    AnnouncedEvent::test_unattested_instance(Event::occur_event_from_dt(
                    start + Duration::seconds(3),
                    ).prefix_path(path!("/time"))),
                    AnnouncedEvent::test_unattested_instance(Event::occur_event_from_dt(
                    start + Duration::seconds(1),
                    ).prefix_path(path!("/time"))),
                    AnnouncedEvent::test_unattested_instance(Event::occur_event_from_dt(
                    start + Duration::seconds(2),
                    ).prefix_path(path!("/time")))
                ];
                // add some time events in the future out of order
                for event in &events {
                    $db.insert_event(event.clone())
                       .await
                       .unwrap();
                }

                let mut stream = time_outcome_stream($event_db);

                // test that they get emitted in order
                let first = stream.next().await.unwrap();
                assert_eq!(
                    first.update.outcome.id,
                    EventId::occur_from_dt(start + Duration::seconds(1)),
                    "first event wasn't the first by expected_outcome_time"
                );
                assert!(now() < start + Duration::milliseconds(1000 + fudge));
                $db.complete_event(
                    &events[1].event.id,
                    Attestation::test_instance(&events[1].event.id),
                )
                   .await
                   .unwrap();
                first.processed_notifier.unwrap().send(false).unwrap();

                let second = stream.next().await.unwrap();
                assert_eq!(
                    second.update.outcome.id,
                    EventId::occur_from_dt(start + Duration::seconds(2)),
                    "second event"
                );
                assert!(now() < start + Duration::milliseconds(2000 + fudge));
                $db.complete_event(
                    &events[2].event.id,
                    Attestation::test_instance(&events[2].event.id),
                )
                   .await
                   .unwrap();
                second.processed_notifier.unwrap().send(false).unwrap();

                let third = stream.next().await.unwrap();
                assert_eq!(
                    third.update.outcome.id,
                    EventId::occur_from_dt(start + Duration::seconds(3)),
                    "third event"
                );
                assert!(now() >= start + Duration::seconds(3));
                assert!(now() < start + Duration::milliseconds(3000 + fudge));
            }
        }
    }
}
