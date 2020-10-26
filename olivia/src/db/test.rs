use super::*;
use crate::core::{PathRef, Schnorr};
use std::str::FromStr;

pub fn test_db(db: &dyn Db<impl Schnorr>) {
    let mut rt = tokio::runtime::Runtime::new().unwrap();
    test_insert_unattested(&mut rt, db);
    test_insert_attested(&mut rt, db);
    test_insert_unattested_then_complete(&mut rt, db);
    test_insert_grandchild_event(&mut rt, db);
    test_child_event_of_node_with_event(&mut rt, db);
    test_get_non_existent_events(&mut rt, db);
    test_multiple_events_on_one_node(&mut rt, db);
}

fn test_insert_unattested(rt: &mut tokio::runtime::Runtime, db: &dyn Db<impl Schnorr>) {
    let unattested_id = EventId::from_str("/test/db/test-insert-unattested?occur").unwrap();
    let obs_event = AnnouncedEvent::test_unattested_instance(unattested_id.clone().into());

    rt.block_on(db.insert_event(obs_event.clone())).unwrap();
    let entry = rt.block_on(db.get_event(&unattested_id)).unwrap().unwrap();

    assert_eq!(
        entry, obs_event,
        "unattested entry retrieved should be same as inserted"
    );

    {
        assert_eq!(
            rt.block_on(db.get_node(PathRef::root().as_str()))
                .unwrap()
                .unwrap()
                .children,
            ["/test"]
        );

        let path = rt.block_on(db.get_node("/test")).unwrap().unwrap();
        assert_eq!(path.events, [""; 0]);
        assert_eq!(path.children[..], ["/test/db".to_string()]);
        assert_eq!(
            rt.block_on(db.get_node("/test/db"))
                .unwrap()
                .unwrap()
                .children[..],
            ["/test/db/test-insert-unattested"]
        );

        let node_path = rt
            .block_on(db.get_node("/test/db/test-insert-unattested"))
            .unwrap()
            .unwrap();
        assert_eq!(node_path.children.len(), 0);
        assert_eq!(node_path.events, [unattested_id]);
    }
}

fn test_insert_attested(rt: &mut tokio::runtime::Runtime, db: &dyn Db<impl Schnorr>) {
    let insert_attested_id = EventId::from_str("/test/db/test-insert-attested?occur").unwrap();
    let obs_event = AnnouncedEvent::test_attested_instance(insert_attested_id.clone().into());

    rt.block_on(db.insert_event(obs_event.clone())).unwrap();
    let entry = rt
        .block_on(db.get_event(&insert_attested_id))
        .unwrap()
        .unwrap();

    assert_eq!(
        entry, obs_event,
        "attested entry retrieved should be same as inserted"
    );

    {
        assert_eq!(
            rt.block_on(db.get_node("/test")).unwrap().unwrap().children[..],
            ["/test/db"],
            "new event did not duplicate parent path"
        );

        let mut children = rt
            .block_on(db.get_node("/test/db"))
            .unwrap()
            .unwrap()
            .children;
        children.sort();

        assert_eq!(
            children[..],
            [
                "/test/db/test-insert-attested",
                "/test/db/test-insert-unattested"
            ]
        );
    }
}

fn test_insert_unattested_then_complete(
    rt: &mut tokio::runtime::Runtime,
    db: &dyn Db<impl Schnorr>,
) {
    let unattested_then_complete_id =
        EventId::from_str("/test/db/test-insert-unattested-then-complete?occur").unwrap();

    let mut obs_event =
        AnnouncedEvent::test_attested_instance(unattested_then_complete_id.clone().into());
    let attestation = obs_event.attestation.take().unwrap();

    rt.block_on(db.insert_event(obs_event.clone())).unwrap();
    rt.block_on(db.complete_event(&unattested_then_complete_id, attestation.clone()))
        .unwrap();

    let entry = rt
        .block_on(db.get_event(&unattested_then_complete_id))
        .unwrap()
        .unwrap();

    obs_event.attestation = Some(attestation);
    assert_eq!(
        entry, obs_event,
        "event should have an attestation after calling complete_event"
    );
}

fn test_insert_grandchild_event(rt: &mut tokio::runtime::Runtime, db: &dyn Db<impl Schnorr>) {
    let grandchild_id = EventId::from_str("/test/db/dbchild/grandchild?occur").unwrap();
    rt.block_on(db.insert_event(AnnouncedEvent::test_attested_instance(
        grandchild_id.clone().into(),
    )))
    .unwrap();

    let mut db_children = rt
        .block_on(db.get_node("/test/db"))
        .unwrap()
        .unwrap()
        .children;

    db_children.sort();

    assert_eq!(
        db_children[..],
        [
            "/test/db/dbchild",
            "/test/db/test-insert-attested",
            "/test/db/test-insert-unattested",
            "/test/db/test-insert-unattested-then-complete",
        ]
    );

    let dbchild = rt
        .block_on(db.get_node("/test/db/dbchild"))
        .unwrap()
        .unwrap();
    assert_eq!(dbchild.events, [""; 0]);
    assert_eq!(dbchild.children[..], ["/test/db/dbchild/grandchild"]);

    let grandchild = rt
        .block_on(db.get_node("/test/db/dbchild/grandchild"))
        .unwrap()
        .unwrap();

    assert_eq!(grandchild.children[..], [""; 0]);
    assert_eq!(grandchild.events[..], [grandchild_id])
}

fn test_child_event_of_node_with_event(
    rt: &mut tokio::runtime::Runtime,
    db: &dyn Db<impl Schnorr>,
) {
    let child = EventId::from_str("/test/db/test-insert-attested/test-sub-event?occur").unwrap();
    rt.block_on(db.insert_event(AnnouncedEvent::test_attested_instance(child.into())))
        .unwrap();
    let parent = rt
        .block_on(db.get_node("/test/db/test-insert-attested"))
        .unwrap()
        .unwrap();

    assert_eq!(
        parent.children,
        ["/test/db/test-insert-attested/test-sub-event"]
    );

    let parent = rt
        .block_on(db.get_node("/test/db/test-insert-attested/test-sub-event"))
        .unwrap()
        .unwrap();

    assert_eq!(
        parent
            .events
            .iter()
            .map(EventId::as_str)
            .collect::<Vec<_>>(),
        ["/test/db/test-insert-attested/test-sub-event?occur"]
    );
}

fn test_get_non_existent_events(rt: &mut tokio::runtime::Runtime, db: &dyn Db<impl Schnorr>) {
    let non_existent = EventId::from_str("/test/db/dont-exist?occur").unwrap();
    assert!(rt.block_on(db.get_event(&non_existent)).unwrap().is_none());
    assert!(rt
        .block_on(db.get_node("/test/db/dont-exist"))
        .unwrap()
        .is_none());
}

fn test_multiple_events_on_one_node(rt: &mut tokio::runtime::Runtime, db: &dyn Db<impl Schnorr>) {
    let first = EventId::from_str("/test/db/RED_BLUE?vs").unwrap();
    let second = EventId::from_str("/test/db/RED_BLUE?left-win").unwrap();

    rt.block_on(db.insert_event(AnnouncedEvent::test_attested_instance(first.clone().into())))
        .unwrap();
    rt.block_on(db.insert_event(AnnouncedEvent::test_attested_instance(
        second.clone().into(),
    )))
    .unwrap();

    let mut red_blue = rt
        .block_on(db.get_node("/test/db/RED_BLUE"))
        .unwrap()
        .unwrap();

    red_blue.events.sort();

    assert_eq!(red_blue.events, [second, first]);
}
