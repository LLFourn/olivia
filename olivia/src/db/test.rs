use super::*;
use olivia_core::{path, Child, ChildDesc, EventKind, Group, Path, PathRef, RangeKind};
use std::str::FromStr;

pub async fn test_db<C: Group>(db: &dyn Db<C>) {
    test_insert_unattested(db).await;
    test_insert_attested(db).await;
    test_insert_unattested_then_complete(db).await;
    test_insert_grandchild_event(db).await;
    test_child_event_of_node_with_event(db).await;
    test_get_non_existent_events(db).await;
    test_multiple_events_on_one_node(db).await;
    test_insert_and_get_public_keys(db).await;
    test_set_node(db).await;
}

macro_rules! assert_children_eq {

    ($children:expr, [ $($child:literal),* $(,)?] $(,$msg:expr)?) => {
        match $children {
            ChildDesc::List { mut list }  => {
                list.sort_unstable_by_key(|child| child.name.clone());
                let list_ref = list.iter().map(|child| &child.name).collect::<Vec<_>>();
                assert_eq!(&list_ref, &[ $($child,)*] as &[&str] $(,$msg)?);
            },
            _ => panic!("children should be a list")
        }
    }
}

async fn test_insert_unattested(db: &dyn Db<impl Group>) {
    let unattested_id = EventId::from_str("/test/db/test-insert-unattested.occur").unwrap();
    let ann_event = AnnouncedEvent::test_unattested_instance(unattested_id.clone().into());

    db.insert_event(ann_event.clone()).await.unwrap();
    let entry = db
        .get_announced_event(&unattested_id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        entry, ann_event,
        "unattested entry retrieved should be same as inserted"
    );

    {
        assert_children_eq!(
            db.get_node(PathRef::root())
                .await
                .unwrap()
                .unwrap()
                .child_desc,
            ["test"]
        );

        let path = db.get_node(path!("/test")).await.unwrap().unwrap();
        assert_eq!(path.events, [EventKind::SingleOccurrence; 0]);
        assert_children_eq!(path.child_desc, ["db"]);
        assert_children_eq!(
            db.get_node(path!("/test/db"))
                .await
                .unwrap()
                .unwrap()
                .child_desc,
            ["test-insert-unattested"]
        );

        let node_path = db
            .get_node(path!("/test/db/test-insert-unattested"))
            .await
            .unwrap()
            .unwrap();
        assert_children_eq!(node_path.child_desc, []);
        assert_eq!(node_path.events, [unattested_id.event_kind()]);
    }
}

async fn test_insert_attested(db: &dyn Db<impl Group>) {
    let insert_attested_id = EventId::from_str("/test/db/test-insert-attested.occur").unwrap();
    let ann_event = AnnouncedEvent::test_attested_instance(insert_attested_id.clone().into());

    db.insert_event(ann_event.clone()).await.unwrap();
    let entry = db
        .get_announced_event(&insert_attested_id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        entry, ann_event,
        "attested entry retrieved should be same as inserted"
    );

    {
        assert_children_eq!(
            db.get_node(path!("/test"))
                .await
                .unwrap()
                .unwrap()
                .child_desc,
            ["db"],
            "new event did not duplicate parent path"
        );

        assert_children_eq!(
            db.get_node(path!("/test/db"))
                .await
                .unwrap()
                .unwrap()
                .child_desc,
            ["test-insert-attested", "test-insert-unattested"]
        );
    }
}

async fn test_insert_unattested_then_complete(db: &dyn Db<impl Group>) {
    let unattested_then_complete_id =
        EventId::from_str("/test/db/test-insert-unattested-then-complete.occur").unwrap();

    let mut ann_event =
        AnnouncedEvent::test_attested_instance(unattested_then_complete_id.clone().into());
    let attestation = ann_event.attestation.take().unwrap();

    db.insert_event(ann_event.clone()).await.unwrap();
    db.complete_event(&unattested_then_complete_id, attestation.clone())
        .await
        .unwrap();

    let entry = db
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

async fn test_insert_grandchild_event(db: &dyn Db<impl Group>) {
    let grandchild_id = EventId::from_str("/test/db/dbchild/grandchild.occur").unwrap();
    db.insert_event(AnnouncedEvent::test_attested_instance(
        grandchild_id.clone().into(),
    ))
    .await
    .unwrap();

    assert_children_eq!(
        db.get_node(path!("/test/db"))
            .await
            .unwrap()
            .unwrap()
            .child_desc,
        [
            "dbchild",
            "test-insert-attested",
            "test-insert-unattested",
            "test-insert-unattested-then-complete",
        ]
    );

    let dbchild = db
        .get_node(path!("/test/db/dbchild"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(dbchild.events, []);
    assert_children_eq!(dbchild.child_desc, ["grandchild"]);

    let grandchild = db
        .get_node(path!("/test/db/dbchild/grandchild"))
        .await
        .unwrap()
        .unwrap();

    assert_children_eq!(grandchild.child_desc, []);
    assert_eq!(grandchild.events[..], [grandchild_id.event_kind()])
}

async fn test_child_event_of_node_with_event(db: &dyn Db<impl Group>) {
    let child = EventId::from_str("/test/db/test-insert-attested/test-sub-event.occur").unwrap();
    db.insert_event(AnnouncedEvent::test_attested_instance(child.into()))
        .await
        .unwrap();
    let parent = db
        .get_node(path!("/test/db/test-insert-attested"))
        .await
        .unwrap()
        .unwrap();

    assert_children_eq!(parent.child_desc, ["test-sub-event"]);

    let parent = db
        .get_node(path!("/test/db/test-insert-attested/test-sub-event"))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(parent.events, [EventKind::SingleOccurrence]);
}

async fn test_get_non_existent_events(db: &dyn Db<impl Group>) {
    let non_existent = EventId::from_str("/test/db/dont-exist.occur").unwrap();
    assert!(db
        .get_announced_event(&non_existent)
        .await
        .unwrap()
        .is_none());
    assert!(db
        .get_node(path!("/test/db/dont-exist"))
        .await
        .unwrap()
        .is_none());
}

async fn test_multiple_events_on_one_node(db: &dyn Db<impl Group>) {
    let first = EventId::from_str("/test/db/RED_BLUE.vs").unwrap();
    let second = EventId::from_str("/test/db/RED_BLUE.winner").unwrap();

    db.insert_event(AnnouncedEvent::test_attested_instance(first.clone().into()))
        .await
        .unwrap();
    db.insert_event(AnnouncedEvent::test_attested_instance(
        second.clone().into(),
    ))
    .await
    .unwrap();

    let mut red_blue = db
        .get_node(path!("/test/db/RED_BLUE"))
        .await
        .unwrap()
        .unwrap();

    red_blue.events.sort();

    assert_eq!(red_blue.events, [first.event_kind(), second.event_kind()]);
}

async fn test_insert_and_get_public_keys<G: Group>(db: &dyn Db<G>) {
    let oracle_keys = G::test_oracle_keys();
    db.set_public_keys(oracle_keys.clone()).await.unwrap();
    let retrieved_keys = db.get_public_keys().await.unwrap().unwrap();
    assert_eq!(oracle_keys, retrieved_keys);
}

async fn test_set_node<G: Group>(db: &dyn Db<G>) {
    let event_ids = vec![
        EventId::from_str("/test/time/2020-09-30T08:00:00.occur").unwrap(),
        EventId::from_str("/test/time/2020-09-30T08:02:00.occur").unwrap(),
        EventId::from_str("/test/time/2020-09-30T08:01:00.occur").unwrap(),
    ];

    let children = event_ids
        .iter()
        .map(|id| Child {
            name: id.path().segments().nth(2).unwrap().to_string(),
            kind: NodeKind::List,
        })
        .collect::<Vec<_>>();

    let events = event_ids
        .iter()
        .map(|id| AnnouncedEvent::test_unattested_instance(id.clone().into()))
        .collect::<Vec<_>>();
    for event in events {
        db.insert_event(event).await.unwrap();
    }

    db.set_node(Node {
        path: Path::from_str("/test/time").unwrap(),
        kind: NodeKind::Range {
            range_kind: RangeKind::Time { interval: 60 },
        },
    })
    .await
    .unwrap();
    assert_eq!(
        db.get_node(path!("/test/time")).await.unwrap().unwrap(),
        GetPath {
            events: vec![],
            child_desc: ChildDesc::Range {
                range_kind: RangeKind::Time { interval: 60 },
                start: Some(children[0].clone()),
                end: Some(children[1].clone())
            }
        }
    );

    db.set_node(Node {
        path: Path::from_str("/test/time").unwrap(),
        kind: NodeKind::List,
    })
    .await
    .unwrap();

    let node = db.get_node(path!("/test/time")).await.unwrap().unwrap();
    match node.child_desc {
        ChildDesc::List { list } => {
            let mut expected = event_ids
                .iter()
                .map(|x| x.path().last())
                .collect::<Vec<_>>();
            expected.sort();
            let mut got = list.iter().map(|x| &x.name).collect::<Vec<_>>();
            got.sort();
            assert_eq!(expected, got);
        }
        _ => panic!("set_node didn't work"),
    }
}
