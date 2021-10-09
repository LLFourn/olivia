#[macro_export]
#[doc(hidden)]
macro_rules! assert_children_eq {
    ($db:expr, $path:expr, children => [ $($child:literal),* $(,)?], events => [ $($event:expr),* $(,)?] $(,$msg:expr)?) => {{
        let mut node = $db.get_node($path).await.unwrap().expect("node should exist");
        match node.child_desc {
            ChildDesc::List { mut list }  => {
                list.sort_unstable_by_key(|child| child.name.clone());
                let list_ref = list.iter().map(|child| &child.name).collect::<Vec<_>>();
                assert_eq!(&list_ref, &[ $($child,)*] as &[&str] $(,$msg)?);
            },
            _ => panic!("children should be a list")
        }
        node.events.sort();
        assert_eq!(node.events, &[ $($event,)*], $(,$msg)?)
    }}
}

#[macro_export]
#[doc(hidden)]
macro_rules! run_node_db_tests {
    (db => $db:ident, curve => $curve:ty, { $($init:tt)* }) => {

        #[allow(redundant_semicolons, unused_imports, unused_variables)]
        mod node_db_test {
            use super::*;
            use olivia_core::{path, Child, ChildDesc, EventKind, Group, Path, PathRef, RangeKind};
            use std::str::FromStr;
            use $crate::assert_children_eq;

            #[tokio::test]
            async fn test_insert_unattested() {
                $($init)*;
                let unattested_id = EventId::from_str("/test/db/test-insert-unattested.occur").unwrap();
                let ann_event = AnnouncedEvent::test_unattested_instance(unattested_id.clone().into());

                $db.insert_event(ann_event.clone()).await.unwrap();
                let entry = $db
                    .get_announced_event(&unattested_id)
                    .await
                    .unwrap()
                    .unwrap();

                assert_eq!(
                    entry, ann_event,
                    "unattested entry retrieved should be same as inserted"
                );

                {
                    assert_children_eq!($db, PathRef::root(), children => ["test"], events => []);
                    assert_children_eq!($db, path!("/test"), children => ["db"], events => []);
                    assert_children_eq!($db, path!("/test/db"), children => ["test-insert-unattested"], events => []);
                    assert_children_eq!($db, path!("/test/db/test-insert-unattested"), children => [], events => [unattested_id.event_kind()]);
                }
            }

            #[tokio::test]
            async fn test_insert_attested() {
                $($init)*;
                let insert_attested_id = EventId::from_str("/test/db/test-insert-attested.occur").unwrap();
                let ann_event = AnnouncedEvent::test_attested_instance(insert_attested_id.clone().into());

                $db.insert_event(ann_event.clone()).await.unwrap();
                let entry = $db
                    .get_announced_event(&insert_attested_id)
                    .await
                    .unwrap()
                    .unwrap();

                assert_eq!(
                    entry, ann_event,
                    "attested entry retrieved should be same as inserted"
                );
            }

            #[tokio::test]
            async fn test_insert_unattested_then_complete() {
                $($init)*;
                let unattested_then_complete_id =
                    EventId::from_str("/test/db/test-insert-unattested-then-complete.occur").unwrap();

                let mut ann_event =
                    AnnouncedEvent::test_attested_instance(unattested_then_complete_id.clone().into());
                let attestation = ann_event.attestation.take().unwrap();

                $db.insert_event(ann_event.clone()).await.unwrap();
                $db.complete_event(&unattested_then_complete_id, attestation.clone())
                   .await
                   .unwrap();

                let entry = $db
                    .get_announced_event(&unattested_then_complete_id)
                    .await
                    .unwrap()
                    .unwrap();

                ann_event.attestation = Some(attestation);
                assert_eq!(
                    entry, ann_event,
                    "event should have an attestation after calling complete_event"
                );
            }

            #[tokio::test]
            async fn test_insert_grandchild_event() {
                $($init)*;
                let grandchild_id = EventId::from_str("/test/db/dbchild/grandchild.occur").unwrap();
                $db.insert_event(AnnouncedEvent::test_attested_instance(
                    grandchild_id.clone().into(),
                ))
                   .await
                   .unwrap();

                assert_children_eq!($db, path!("/test/db"), children => ["dbchild"], events => []);
                assert_children_eq!($db, path!("/test/db"), children => ["dbchild"], events => []);
                assert_children_eq!($db, path!("/test/db/dbchild"), children => ["grandchild"], events => []);
                assert_children_eq!($db, path!("/test/db/dbchild/grandchild"), children => [], events => [grandchild_id.event_kind()]);
            }

            #[tokio::test]
            async fn test_child_event_of_node_with_event() {
                $($init)*;

                $db.insert_event(AnnouncedEvent::test_attested_instance(EventId::from_str("/test/db/an-event.occur").unwrap().into()))
                   .await
                   .unwrap();

                $db.insert_event(AnnouncedEvent::test_attested_instance(EventId::from_str("/test/db/an-event/a-sub-event.occur").unwrap().into()))
                   .await
                   .unwrap();

                assert_children_eq!($db, path!("/test/db/an-event"), children => ["a-sub-event"], events => [EventKind::SingleOccurrence]);
            }

            #[tokio::test]
            async fn test_get_non_existent_events() {
                $($init)*;
                let non_existent = EventId::from_str("/test/db/dont-exist.occur").unwrap();
                assert!($db
                        .get_announced_event(&non_existent)
                        .await
                        .unwrap()
                        .is_none());
                assert!($db
                        .get_node(path!("/test/db/dont-exist"))
                        .await
                        .unwrap()
                        .is_none());
            }

            #[tokio::test]
            async fn test_multiple_events_on_one_node() {
                $($init)*;
                let first = EventId::from_str("/test/db/RED_BLUE.vs").unwrap();
                let second = EventId::from_str("/test/db/RED_BLUE.winner").unwrap();

                $db.insert_event(AnnouncedEvent::test_attested_instance(first.clone().into()))
                   .await
                   .unwrap();
                $db.insert_event(AnnouncedEvent::test_attested_instance(
                    second.clone().into(),
                ))
                   .await
                   .unwrap();

                assert_children_eq!($db, path!("/test/db/RED_BLUE"), children => [], events =>[first.event_kind(), second.event_kind()]);
            }

            #[tokio::test]
            async fn test_insert_and_get_public_keys() {
                $($init)*;
                let oracle_keys = <$curve>::test_oracle_keys();
                $db.set_public_keys(oracle_keys.clone()).await.unwrap();
                let retrieved_keys = $db.get_public_keys().await.unwrap().unwrap();
                assert_eq!(oracle_keys, retrieved_keys);
            }

            #[tokio::test]
            async fn test_set_node() {
                $($init)*;
                let event_ids = vec![
                    EventId::from_str("/test/time/2020-09-30T08:00:00/foo.occur").unwrap(),
                    EventId::from_str("/test/time/2020-09-30T08:02:00/bar.occur").unwrap(),
                    EventId::from_str("/test/time/2020-09-30T08:01:00/baz.occur").unwrap(),
                ];

                let times = event_ids.iter().map(|id|id.path().segments().nth(2).unwrap().to_string() ).collect::<Vec<_>>();
                let events = event_ids
                    .iter()
                    .zip(times.iter())
                    .map(|(id,time)| AnnouncedEvent::test_unattested_instance( Event {
                        id: id.clone(),
                        expected_outcome_time: olivia_core::chrono::NaiveDateTime::from_str(time).ok(),
                    }))
                    .collect::<Vec<_>>();
                for event in events {
                    $db.insert_event(event).await.unwrap();
                }

                $db.set_node(Node {
                    path: Path::from_str("/test/time").unwrap(),
                    kind: NodeKind::Range {
                        range_kind: RangeKind::Time { interval: 60 },
                    },
                })
                   .await
                   .unwrap();

                assert_eq!(
                    $db.get_node(path!("/test/time")).await.unwrap().unwrap(),
                    GetPath {
                        events: vec![],
                        child_desc: ChildDesc::Range {
                            range_kind: RangeKind::Time { interval: 60 },
                            start: Some(times[0].clone()),
                            next_unattested: Some(times[0].clone()),
                            end: Some(times[1].clone())
                        }
                    },
                    "none are attested so next should be first",
                );

                $db.complete_event(&event_ids[0], Attestation::test_instance(&event_ids[0])).await.unwrap();

                assert_eq!(
                    $db.get_node(path!("/test/time")).await.unwrap().unwrap(),
                    GetPath {
                        events: vec![],
                        child_desc: ChildDesc::Range {
                            range_kind: RangeKind::Time { interval: 60 },
                            start: Some(times[0].clone()),
                            next_unattested: Some(times[2].clone()),
                            end: Some(times[1].clone())
                        }
                    },
                    "after attesting event 'next' changes"
                );

                $db.set_node(Node {
                    path: Path::from_str("/test/time").unwrap(),
                    kind: NodeKind::List,
                })
                   .await
                   .unwrap();

                let node = $db.get_node(path!("/test/time")).await.unwrap().unwrap();
                match node.child_desc {
                    ChildDesc::List { list } => {
                        let mut expected = times.clone();
                        expected.sort();
                        let mut got = list.iter().map(|x| x.name.clone()).collect::<Vec<_>>();
                        got.sort();
                        assert_eq!(expected, got);
                    }
                    _ => panic!("set_node didn't work"),
                }
            }
        }
    }
}

#[macro_export]
#[doc(hidden)]
macro_rules!  run_query_db_tests {
	(db => $db:ident, curve => $curve:ty, { $($init:tt)* }) => {

        #[allow(redundant_semicolons, unused_imports, unused_variables)]
        mod query_db_test {
            use super::*;
            use olivia_core::{path, Child, ChildDesc, EventKind, Group, Path, PathRef, RangeKind, chrono::NaiveDateTime};
            use std::str::FromStr;

            macro_rules! row {
                ($time:literal, $prefix:expr) => {{
                    use olivia_core::PrefixPath;
                    let time = NaiveDateTime::from_str($time).expect("valid time");
                    let mut ann_event = AnnouncedEvent::test_unattested_instance(Event::occur_event_from_dt(time).prefix_path($prefix));
                    ann_event.event.expected_outcome_time = Some(time);
                    ann_event
                }};
                ($time:literal, $prefix:expr, attested) => {{
                    use olivia_core::PrefixPath;
                    let time = NaiveDateTime::from_str($time).expect("valid time");
                    let mut ann_event = AnnouncedEvent::test_attested_instance(Event::occur_event_from_dt(time).prefix_path($prefix));
                    ann_event.event.expected_outcome_time = Some(time);
                    ann_event.attestation.as_mut().unwrap().time = NaiveDateTime::from_str($time).unwrap();
                    ann_event
                }}
            }

            #[tokio::test]
            async fn earliest_and_latest() {
                use crate::db::NodeKind;
                use olivia_core::{path, Child, ChildDesc, EventKind, Group, Path, PathRef, RangeKind};
                $($init)*;

                $db.insert_event({
                    // put in a non time event which *SHOULD* be ignored
                    let time = NaiveDateTime::from_str("1997-03-01T00:01:00").unwrap();
                    let mut ann_event = AnnouncedEvent::test_unattested_instance(
                        EventId::from_str("/foo/bar/baz.occur").unwrap().into(),
                    );
                    ann_event.event.expected_outcome_time = Some(time);
                    ann_event
                }).await.unwrap();

                for (top,prefix) in vec![(path!("/time"), path!("/time")), (path!("/time2"), path!("/time2/nested/deeper")) ] {
                    let test_data = vec![
                        row!("2020-03-01T00:25:00", prefix),
                        row!("2020-03-01T00:30:00", prefix),
                        row!("2020-03-01T00:20:00", prefix),
                        row!("2020-03-01T00:10:00", prefix, attested),
                        row!("2020-03-01T00:05:00", prefix, attested),
                        row!("2020-03-01T00:15:00", prefix, attested)
                    ];

                    for event in test_data.iter() {
                        $db.insert_event(event.clone()).await.unwrap();
                    }

                    let latest_time_event = $db
                        .query_event(EventQuery { path: Some(top), order: Order::Latest, ..Default::default() })
                        .await
                        .expect("latest_time_event isn't Err")
                        .expect("latest_time_event isn't None");

                    assert_eq!(latest_time_event, test_data[1].event, "latest_time_event");

                    let earliest_unattested_time_event = $db
                        .query_event(EventQuery { path: Some(top), order: Order::Earliest, attested: Some(false), ..Default::default() })
                        .await
                        .expect("earliest_unattested_time_event isn't Err")
                        .expect("earliest_unattested_time_event isn't None");

                    assert_eq!(earliest_unattested_time_event, test_data[2].event, "earliest_unattested_time_event");
                }

                let earliest_event = $db.query_event(EventQuery { path: Some(PathRef::root()), order: Order::Earliest, ..Default::default() }).await.unwrap().unwrap();
                assert_eq!(earliest_event.id.as_str(), "/foo/bar/baz.occur");
                assert_eq!(earliest_event, $db.query_event(EventQuery { order: Order::Earliest, ..Default::default() }).await.unwrap().unwrap())
            }
        }
	};
}
