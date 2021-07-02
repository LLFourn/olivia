use super::*;
use olivia_core::{ChildDesc, Group, PathRef};
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
}

macro_rules! assert_children_eq {

    ($children:expr, [ $($child:literal),* $(,)?] $(,$msg:expr)?) => {
        match $children {
            ChildDesc::List { mut list }  => { list.sort(); assert_eq!(&list, &[ $($child,)*] as &[&str] $(,$msg)?); } ,
            _ => panic!("children should be a list")
        }
    }
}

async fn test_insert_unattested(db: &dyn Db<impl Group>) {
    let unattested_id = EventId::from_str("/test/db/test-insert-unattested.occur").unwrap();
    let obs_event = AnnouncedEvent::test_unattested_instance(unattested_id.clone().into());

    db.insert_event(obs_event.clone()).await.unwrap();
    let entry = db.get_event(&unattested_id).await.unwrap().unwrap();

    assert_eq!(
        entry, obs_event,
        "unattested entry retrieved should be same as inserted"
    );

    {
        assert_children_eq!(
            db.get_node(PathRef::root().as_str())
                .await
                .unwrap()
                .unwrap()
                .child_desc,
            ["test"]
        );

        let path = db.get_node("/test").await.unwrap().unwrap();
        assert_eq!(path.events, [""; 0]);
        assert_children_eq!(path.child_desc, ["db"]);
        assert_children_eq!(
            db.get_node("/test/db").await.unwrap().unwrap().child_desc,
            ["test-insert-unattested"]
        );

        let node_path = db
            .get_node("/test/db/test-insert-unattested")
            .await
            .unwrap()
            .unwrap();
        assert_children_eq!(node_path.child_desc, []);
        assert_eq!(node_path.events, [unattested_id]);
    }
}

async fn test_insert_attested(db: &dyn Db<impl Group>) {
    let insert_attested_id = EventId::from_str("/test/db/test-insert-attested.occur").unwrap();
    let obs_event = AnnouncedEvent::test_attested_instance(insert_attested_id.clone().into());

    db.insert_event(obs_event.clone()).await.unwrap();
    let entry = db.get_event(&insert_attested_id).await.unwrap().unwrap();

    assert_eq!(
        entry, obs_event,
        "attested entry retrieved should be same as inserted"
    );

    {
        assert_children_eq!(
            db.get_node("/test").await.unwrap().unwrap().child_desc,
            ["db"],
            "new event did not duplicate parent path"
        );

        assert_children_eq!(
            db.get_node("/test/db").await.unwrap().unwrap().child_desc,
            ["test-insert-attested", "test-insert-unattested"]
        );
    }
}

async fn test_insert_unattested_then_complete(db: &dyn Db<impl Group>) {
    let unattested_then_complete_id =
        EventId::from_str("/test/db/test-insert-unattested-then-complete.occur").unwrap();

    let mut obs_event =
        AnnouncedEvent::test_attested_instance(unattested_then_complete_id.clone().into());
    let attestation = obs_event.attestation.take().unwrap();

    db.insert_event(obs_event.clone()).await.unwrap();
    db.complete_event(&unattested_then_complete_id, attestation.clone())
        .await
        .unwrap();

    let entry = db
        .get_event(&unattested_then_complete_id)
        .await
        .unwrap()
        .unwrap();

    obs_event.attestation = Some(attestation);
    assert_eq!(
        entry, obs_event,
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
        db.get_node("/test/db").await.unwrap().unwrap().child_desc,
        [
            "dbchild",
            "test-insert-attested",
            "test-insert-unattested",
            "test-insert-unattested-then-complete",
        ]
    );

    let dbchild = db.get_node("/test/db/dbchild").await.unwrap().unwrap();
    assert_eq!(dbchild.events, [""; 0]);
    assert_children_eq!(dbchild.child_desc, ["grandchild"]);

    let grandchild = db
        .get_node("/test/db/dbchild/grandchild")
        .await
        .unwrap()
        .unwrap();

    assert_children_eq!(grandchild.child_desc, []);
    assert_eq!(grandchild.events[..], [grandchild_id])
}

async fn test_child_event_of_node_with_event(db: &dyn Db<impl Group>) {
    let child = EventId::from_str("/test/db/test-insert-attested/test-sub-event.occur").unwrap();
    db.insert_event(AnnouncedEvent::test_attested_instance(child.into()))
        .await
        .unwrap();
    let parent = db
        .get_node("/test/db/test-insert-attested")
        .await
        .unwrap()
        .unwrap();

    assert_children_eq!(
        parent.child_desc,
        ["test-sub-event"]
    );

    let parent = db
        .get_node("/test/db/test-insert-attested/test-sub-event")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        parent
            .events
            .iter()
            .map(EventId::as_str)
            .collect::<Vec<_>>(),
        ["/test/db/test-insert-attested/test-sub-event.occur"]
    );
}

async fn test_get_non_existent_events(db: &dyn Db<impl Group>) {
    let non_existent = EventId::from_str("/test/db/dont-exist.occur").unwrap();
    assert!(db.get_event(&non_existent).await.unwrap().is_none());
    assert!(db.get_node("/test/db/dont-exist").await.unwrap().is_none());
}

async fn test_multiple_events_on_one_node(db: &dyn Db<impl Group>) {
    let first = EventId::from_str("/test/db/RED_BLUE.vs").unwrap();
    let second = EventId::from_str("/test/db/RED_BLUE.win").unwrap();

    db.insert_event(AnnouncedEvent::test_attested_instance(first.clone().into()))
        .await
        .unwrap();
    db.insert_event(AnnouncedEvent::test_attested_instance(
        second.clone().into(),
    ))
    .await
    .unwrap();

    let mut red_blue = db.get_node("/test/db/RED_BLUE").await.unwrap().unwrap();

    red_blue.events.sort();

    assert_eq!(red_blue.events, [first, second]);
}

async fn test_insert_and_get_public_keys<G: Group>(db: &dyn Db<G>) {
    let oracle_keys = G::test_oracle_keys();
    db.set_public_keys(oracle_keys.clone()).await.unwrap();
    let retrieved_keys = db.get_public_keys().await.unwrap().unwrap();
    assert_eq!(oracle_keys, retrieved_keys);
}
