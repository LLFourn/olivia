use crate::{
    core::{AnnouncedEvent, Attestation, Event, EventId, Curve},
};
pub mod diesel;
pub mod in_memory;
use async_trait::async_trait;
#[cfg(test)]
pub mod test;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub events: Vec<EventId>,
    pub children: Vec<String>,
}

#[async_trait]
pub trait DbRead<C: Curve>: Send + Sync {
    async fn get_event(&self, id: &EventId) -> Result<Option<AnnouncedEvent<C>>, Error>;
    async fn get_node(&self, path: &str) -> Result<Option<Item>, Error>;
}

#[async_trait]
pub trait DbWrite<C: Curve>: Send + Sync {
    async fn insert_event(&self, observed_event: AnnouncedEvent<C>) -> Result<(), Error>;
    async fn complete_event(&self, event_id: &EventId, outcome: Attestation<C>) -> Result<(), Error>;
}

#[async_trait]
pub trait DbMeta<C: Curve>: Send + Sync {
    async fn get_public_key(&self) -> Result<Option<C::PublicKey>, Error>;
    async fn set_public_key(&self, public_key: C::PublicKey) -> Result<(), Error>;
}

#[async_trait]
pub trait TimeTickerDb {
    async fn latest_time_event(&self) -> Result<Option<Event>, Error>;
    async fn earliest_unattested_time_event(&self) -> Result<Option<Event>, Error>;
}

pub trait Db<C: Curve>: DbRead<C> + DbWrite<C> + TimeTickerDb + DbMeta<C> {  }
