use olivia_core::{AnnouncedEvent, Attestation, Event, EventId, Group, NodeKind, OracleKeys, PathNode};
pub mod in_memory;
pub mod postgres;
use async_trait::async_trait;
use olivia_core::EventKind;
#[cfg(test)]
pub mod test;

pub type Error = anyhow::Error;

#[async_trait]
pub trait DbRead<C: Group>: Send + Sync {
    async fn get_event(&self, id: &EventId) -> anyhow::Result<Option<AnnouncedEvent<C>>>;
    async fn get_node(&self, path: &str) -> anyhow::Result<Option<PathNode>>;
    async fn latest_child_event(
        &self,
        path: &str,
        kind: EventKind,
    ) -> anyhow::Result<Option<Event>>;
    async fn earliest_unattested_child_event(
        &self,
        path: &str,
        kind: EventKind,
    ) -> anyhow::Result<Option<Event>>;
    async fn get_public_keys(&self) -> Result<Option<OracleKeys<C>>, Error>;
}

#[async_trait]
pub trait DbWrite<C: Group>: Send + Sync {
    async fn insert_event(&self, observed_event: AnnouncedEvent<C>) -> Result<(), Error>;
    async fn set_node_kind(&self, path: &str, kind: NodeKind) -> Result<(), Error>;
    async fn complete_event(
        &self,
        event_id: &EventId,
        outcome: Attestation<C>,
    ) -> Result<(), Error>;

    async fn set_public_keys(&self, public_key: OracleKeys<C>) -> Result<(), Error>;
}

pub trait Db<C: Group>: DbRead<C> + DbWrite<C> + Send + Sync + 'static + BorrowDb<C> {}

pub trait BorrowDb<C>: Send + Sync + 'static {
    fn borrow_db(&self) -> &dyn Db<C>;
}

impl<C: Group> BorrowDb<C> for std::sync::Arc<dyn Db<C>> {
    fn borrow_db(&self) -> &dyn Db<C> {
        self.as_ref()
    }
}
