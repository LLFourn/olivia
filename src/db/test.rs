use super::*;
use crate::{
    core::{Event, Outcome, PathRef},
    keychain::KeyChain,
    seed::Seed,
};
use std::str::FromStr;

const TEST_SEED: Seed = Seed::new([42u8; 64]);

impl Attestation {
    pub fn test_new(event_id: &EventId) -> Self {
        let outcome = Outcome::test_new(event_id);
        Attestation::new(
            format!("{}", outcome.outcome),
            chrono::Utc::now().naive_utc(),
            KeyChain::new(TEST_SEED).scalars_for_event_outcome(&outcome),
        )
    }
}

impl Outcome {
    pub fn test_new(event_id: &EventId) -> Self {
        Outcome {
            event_id: event_id.clone(),
            time: chrono::Utc::now().naive_utc(),
            outcome: event_id.default_outcome(),
        }
    }
}

impl ObservedEvent {
    pub fn test_new(id: &EventId) -> Self {
        let event = Event {
            id: id.clone(),
            expected_outcome_time: None,
        };
        ObservedEvent {
            event: event.clone(),
            nonce: KeyChain::new(TEST_SEED).nonces_for_event(&event.id).into(),
            attestation: Some(Attestation::test_new(id)),
        }
    }
}

impl From<Event> for ObservedEvent {
    fn from(event: Event) -> Self {
        let nonce = KeyChain::new(TEST_SEED).nonces_for_event(&event.id).into();
        ObservedEvent {
            event,
            nonce,
            attestation: None,
        }
    }
}

pub fn test_db(db: &dyn Db) {
    let mut rt = tokio::runtime::Runtime::new().unwrap();
    test_insert_unattested(&mut rt, db);
    test_insert_attested(&mut rt, db);
    test_insert_unattested_then_complete(&mut rt, db);
    test_insert_grandchild_event(&mut rt, db);
    test_child_event_of_node_with_event(&mut rt, db);
    test_get_non_existent_events(&mut rt, db);
    test_multiple_events_on_one_node(&mut rt, db);
}

fn test_insert_unattested(rt: &mut tokio::runtime::Runtime, db: &dyn Db) {
    let unattested_id = EventId::from_str("test/db/test-insert-unattested.occur").unwrap();
    let mut obs_event = ObservedEvent::test_new(&unattested_id);
    obs_event.attestation = None;

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
            ["test"]
        );

        let path = rt.block_on(db.get_node("test")).unwrap().unwrap();
        assert_eq!(path.events, []);
        assert_eq!(path.children[..], ["test/db".to_string()]);
        assert_eq!(
            rt.block_on(db.get_node("test/db"))
                .unwrap()
                .unwrap()
                .children[..],
            ["test/db/test-insert-unattested"]
        );

        let node_path = rt
            .block_on(db.get_node("test/db/test-insert-unattested"))
            .unwrap()
            .unwrap();
        assert_eq!(node_path.children.len(), 0);
        assert_eq!(node_path.events, [unattested_id]);
    }
}

fn test_insert_attested(rt: &mut tokio::runtime::Runtime, db: &dyn Db) {
    let insert_attested_id = EventId::from_str("test/db/test-insert-attested.occur").unwrap();
    let obs_event = ObservedEvent::test_new(&insert_attested_id);
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
            rt.block_on(db.get_node("test")).unwrap().unwrap().children[..],
            ["test/db"],
            "new event did not duplicate parent path"
        );

        let mut children = rt
            .block_on(db.get_node("test/db"))
            .unwrap()
            .unwrap()
            .children;
        children.sort();

        assert_eq!(
            children[..],
            [
                "test/db/test-insert-attested",
                "test/db/test-insert-unattested"
            ]
        );
    }
}

fn test_insert_unattested_then_complete(rt: &mut tokio::runtime::Runtime, db: &dyn Db) {
    let unattested_then_complete_id =
        EventId::from_str("test/db/test-insert-unattested-then-complete.occur").unwrap();

    let mut obs_event = ObservedEvent::test_new(&unattested_then_complete_id);
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

fn test_insert_grandchild_event(rt: &mut tokio::runtime::Runtime, db: &dyn Db) {
    let grandchild_id = EventId::from_str("test/db/dbchild/grandchild.occur").unwrap();
    rt.block_on(db.insert_event(ObservedEvent::test_new(&grandchild_id)))
        .unwrap();

    let mut db_children = rt
        .block_on(db.get_node("test/db"))
        .unwrap()
        .unwrap()
        .children;

    db_children.sort();

    assert_eq!(
        db_children[..],
        [
            "test/db/dbchild",
            "test/db/test-insert-attested",
            "test/db/test-insert-unattested",
            "test/db/test-insert-unattested-then-complete",
        ]
    );

    let dbchild = rt
        .block_on(db.get_node("test/db/dbchild"))
        .unwrap()
        .unwrap();
    assert_eq!(dbchild.events, []);
    assert_eq!(dbchild.children[..], ["test/db/dbchild/grandchild"]);

    let grandchild = rt
        .block_on(db.get_node("test/db/dbchild/grandchild"))
        .unwrap()
        .unwrap();

    assert_eq!(grandchild.children[..], [""; 0]);
    assert_eq!(grandchild.events[..], [grandchild_id])
}

fn test_child_event_of_node_with_event(rt: &mut tokio::runtime::Runtime, db: &dyn Db) {
    let child = EventId::from_str("test/db/test-insert-attested/test-sub-event.occur").unwrap();
    rt.block_on(db.insert_event(ObservedEvent::test_new(&child)))
        .unwrap();
    let parent = rt
        .block_on(db.get_node("test/db/test-insert-attested"))
        .unwrap()
        .unwrap();

    assert_eq!(
        parent.children,
        ["test/db/test-insert-attested/test-sub-event"]
    );
}

fn test_get_non_existent_events(rt: &mut tokio::runtime::Runtime, db: &dyn Db) {
    let non_existent = EventId::from_str("test/db/dont-exist.occur").unwrap();
    assert!(rt.block_on(db.get_event(&non_existent)).unwrap().is_none());
    assert!(rt
        .block_on(db.get_node("test/db/dont-exist"))
        .unwrap()
        .is_none());
}

fn test_multiple_events_on_one_node(rt: &mut tokio::runtime::Runtime, db: &dyn Db) {
    let first = EventId::from_str("test/db/RED_BLUE.vs").unwrap();
    let second = EventId::from_str("test/db/RED_BLUE.left-win").unwrap();

    rt.block_on(db.insert_event(ObservedEvent::test_new(&first)))
        .unwrap();
    rt.block_on(db.insert_event(ObservedEvent::test_new(&second)))
        .unwrap();

    let mut red_blue = rt
        .block_on(db.get_node("test/db/RED_BLUE"))
        .unwrap()
        .unwrap();

    red_blue.events.sort();

    assert_eq!(red_blue.events, [second, first]);
}
