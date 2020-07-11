use crate::{
    event::{Attestation, Event, EventId, ObservedEvent, PathRef},
    oracle,
};
pub mod diesel;
pub mod in_memory;
use async_trait::async_trait;
#[cfg(test)]
pub mod test;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Item {
    pub event: Option<ObservedEvent>,
    pub children: Vec<String>,
}

#[async_trait]
pub trait DbRead: Send + Sync {
    async fn get_event(&self, id: &EventId) -> Result<Option<ObservedEvent>, Error>;
    async fn get_path(&self, path: PathRef<'_>) -> Result<Option<Item>, Error>;
}

#[async_trait]
pub trait DbWrite: Send + Sync {
    async fn insert_event(&self, observed_event: ObservedEvent) -> Result<(), Error>;
    async fn complete_event(&self, event_id: &EventId, outcome: Attestation) -> Result<(), Error>;
}

#[async_trait]
pub trait DbMeta: Send + Sync {
    async fn get_public_keys(&self) -> Result<Option<oracle::OraclePubkeys>, Error>;
    async fn set_public_keys(&self, public_keys: oracle::OraclePubkeys) -> Result<(), Error>;
}

#[async_trait]
pub trait TimeTickerDb {
    async fn latest_time_event(&self) -> Result<Option<Event>, Error>;
    async fn earliest_unattested_time_event(&self) -> Result<Option<Event>, Error>;
}

pub trait Db: DbRead + DbWrite + TimeTickerDb + DbMeta {}
