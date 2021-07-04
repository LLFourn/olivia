#[macro_export]
#[doc(hidden)]
macro_rules! run_time_db_tests {
    (db => $db:ident, curve => $curve:ty, { $($init:tt)* }) => {

        #[allow(redundant_semicolons, unused_imports)]
        mod time_db_test {
            use super::*;
            use olivia_core::{AnnouncedEvent, EventKind, Event, EventId};
            use chrono::{NaiveDateTime, Duration};
            use crate::sources::time_ticker::*;
            use core::str::FromStr;
            use futures::stream::StreamExt;
            fn logger() -> slog::Logger {
                slog::Logger::root(slog::Discard, o!())
            }

            fn now() -> NaiveDateTime {
                chrono::Utc::now().naive_utc()
            }


            #[tokio::test]
            async fn test_time_range_db() {
                use crate::db::NodeKind;
                use olivia_core::{RangeKind, ChildDesc};
                $($init)*;

                let test_data = vec![
                    {
                        let time = NaiveDateTime::from_str("2020-03-01T00:25:00").unwrap();
                        let mut obs_event = AnnouncedEvent::test_unattested_instance(Event::occur_event_from_dt(time));
                        obs_event.event.expected_outcome_time = Some(time);
                        obs_event
                    },
                    {
                        let time = NaiveDateTime::from_str("2020-03-01T00:30:00").unwrap();
                        let mut obs_event = AnnouncedEvent::test_unattested_instance(Event::occur_event_from_dt(time));
                        obs_event.event.expected_outcome_time = Some(time);
                        obs_event
                    },
                    {
                        let time = NaiveDateTime::from_str("2020-03-01T00:20:00").unwrap();
                        let mut obs_event = AnnouncedEvent::test_unattested_instance(Event::occur_event_from_dt(time));
                        obs_event.event.expected_outcome_time = Some(time);
                        obs_event
                    },
                    {
                        // put in a non time event which *SHOULD* be ignored
                        let time = NaiveDateTime::from_str("2020-03-01T00:11:00").unwrap();
                        let mut obs_event = AnnouncedEvent::test_unattested_instance(
                            EventId::from_str("/foo/bar/baz.occur").unwrap().into(),
                        );
                        obs_event.event.expected_outcome_time = Some(time);
                        obs_event
                    },
                    {
                        let time = NaiveDateTime::from_str("2020-03-01T00:10:00").unwrap();
                        let mut obs_event = AnnouncedEvent::test_attested_instance(Event::occur_event_from_dt(time));
                        obs_event.event.expected_outcome_time = Some(time);
                        obs_event.attestation.as_mut().unwrap().time = time;
                        obs_event
                    },
                    {
                        let time = NaiveDateTime::from_str("2020-03-01T00:05:00").unwrap();
                        let mut obs_event = AnnouncedEvent::test_attested_instance(Event::occur_event_from_dt(time));
                        obs_event.event.expected_outcome_time = Some(time);
                        obs_event.attestation.as_mut().unwrap().time = time;
                        obs_event
                    },
                    {
                        let time = NaiveDateTime::from_str("2020-03-01T00:15:00").unwrap();
                        let mut obs_event = AnnouncedEvent::test_attested_instance(Event::occur_event_from_dt(time));
                        obs_event.event.expected_outcome_time = Some(time);
                        obs_event.attestation.as_mut().unwrap().time = time;
                        obs_event
                    },
                ];

                for event in test_data.iter() {
                    $db.insert_event(event.clone()).await.unwrap();
                }

                $db.set_node_kind(
                    "/time",
                    NodeKind::Range {
                        range_kind: RangeKind::Time { interval: 60 },
                    },
                )
                   .await
                   .unwrap();

                let root_node = $db.get_node("/").await.unwrap().expect("root exists");
                if let ChildDesc::List { list } = root_node.child_desc {
                    assert_eq!(list.len(), 2);
                    let time = list.iter().find(|child| &child.name == "time").unwrap();
                    assert_eq!(time.kind, NodeKind::Range { range_kind: RangeKind::Time { interval: 60 } });
                }
                else {
                    panic!("root should be list kind")
                }

                let latest_time_event = $db
                    .latest_child_event("/time", EventKind::SingleOccurrence)
                    .await
                    .expect("latest_time_event isn't Err")
                    .expect("latest_time_event isn't None");

                assert_eq!(latest_time_event, test_data[1].event);

                let earliest_unattested_time_event = $db
                    .earliest_unattested_child_event("/time", EventKind::SingleOccurrence)
                    .await
                    .expect("earliest_unattested_time_event isn't Err")
                    .expect("earliest_unattested_time_event isn't None");

                assert_eq!(earliest_unattested_time_event, test_data[2].event);

                match $db.get_node("/time").await.unwrap().unwrap().child_desc {
                    ChildDesc::Range {
                        range_kind,
                        start,
                        end,
                    } => {
                        assert_eq!(start, Some(Child { name: "2020-03-01T00:05:00".into(), kind: NodeKind::List }));
                        assert_eq!(end, Some(Child { name:  "2020-03-01T00:30:00".into(), kind: NodeKind::List }));
                        assert_eq!(range_kind, RangeKind::Time { interval: 60 });
                    }
                    _ => panic!("should be a range"),
                }
            }

            #[tokio::test]
            async fn time_ticker_events_stream() {
                $($init)*;
                let look_ahead = Duration::seconds(2);
                let interval = Duration::seconds(1);
                let start = now();
                let mut stream =
                    time_events_stream($db.clone(), look_ahead, interval, start, logger()).boxed();
                let mut cur = start.clone();

                {
                    let update = stream.next().await.expect("Not None");
                    let event = update.update;
                    assert_eq!(event.id, EventId::occur_from_dt(cur));
                    $db.insert_event(AnnouncedEvent::test_unattested_instance(event))
                       .await
                       .unwrap();
                    let _ = update.processed_notifier.unwrap().send(());
                }

                cur += interval;

                {
                    let update = stream.next().await.expect("Not None");
                    let event = update.update;
                    assert_eq!(event.id, EventId::occur_from_dt(cur));
                    $db.insert_event(AnnouncedEvent::test_unattested_instance(event))
                       .await
                       .unwrap();
                    let _ = update.processed_notifier.unwrap().send(());
                }

                cur += interval;

                {
                    let update = stream.next().await.expect("Not None");
                    let event = update.update;
                    assert_eq!(event.id, EventId::occur_from_dt(cur));
                    $db.insert_event(AnnouncedEvent::test_unattested_instance(event))
                       .await
                       .unwrap();
                    let _ = update.processed_notifier.unwrap().send(());
                }
                assert!(
                    now() < start + Duration::milliseconds(100),
                    "we shouldn't have waited for anything yet"
                );

                cur += interval;
                {
                    let update = stream.next().await.expect("Not None");
                    let event = update.update;
                    assert_eq!(event.id, EventId::occur_from_dt(cur));
                    $db.insert_event(AnnouncedEvent::test_unattested_instance(event))
                       .await
                       .unwrap();
                    let _ = update.processed_notifier.unwrap().send(());
                }

                assert!(
                    now() > start + Duration::seconds(1),
                    "we should have waited for 1 second"
                );
                assert!(
                    now() < start + Duration::milliseconds(1200),
                    "shouldn't have waited too much"
                );
            }

            #[tokio::test]
            async fn time_ticker_outcome_empty_db() {
                $($init)*;
                let mut stream = time_outcomes_stream($db, logger()).boxed();
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
                let mut stream = time_outcomes_stream($db, logger()).boxed();
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
                )))
                   .await
                   .unwrap();

                let mut stream = time_outcomes_stream($db, logger()).boxed();
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

                // add some time events in the future out of order
                $db.insert_event(AnnouncedEvent::test_unattested_instance(Event::occur_event_from_dt(
                    start + Duration::seconds(3),
                )))
                   .await
                   .unwrap();

                $db.insert_event(AnnouncedEvent::test_unattested_instance(Event::occur_event_from_dt(
                    start + Duration::seconds(1),
                )))
                   .await
                   .unwrap();

                $db.insert_event(AnnouncedEvent::test_unattested_instance(Event::occur_event_from_dt(
                    start + Duration::seconds(2),
                )))
                   .await
                   .unwrap();

                let mut stream = time_outcomes_stream($db.clone(), logger()).boxed();

                // test that they get emitted in order
                let first = stream.next().await.unwrap();
                assert_eq!(
                    first.update.outcome.id,
                    EventId::occur_from_dt(start + Duration::seconds(1)),
                    "first event wasn't the first by expected_outcome_time"
                );
                assert!(now() < start + Duration::milliseconds(1000 + fudge));
                $db.complete_event(
                    &first.update.outcome.id,
                    Attestation::test_instance(&first.update.outcome.id),
                )
                   .await
                   .unwrap();
                first.processed_notifier.unwrap().send(()).unwrap();

                let second = stream.next().await.unwrap();
                assert_eq!(
                    second.update.outcome.id,
                    EventId::occur_from_dt(start + Duration::seconds(2))
                );
                assert!(now() < start + Duration::milliseconds(2000 + fudge));
                $db.complete_event(
                    &second.update.outcome.id,
                    Attestation::test_instance(&first.update.outcome.id),
                )
                   .await
                   .unwrap();
                second.processed_notifier.unwrap().send(()).unwrap();

                let third = stream.next().await.unwrap();
                assert_eq!(
                    third.update.outcome.id,
                    EventId::occur_from_dt(start + Duration::seconds(3))
                );
                assert!(now() >= start + Duration::seconds(3));
                assert!(now() < start + Duration::milliseconds(3000 + fudge));
            }
        }
    }
}
