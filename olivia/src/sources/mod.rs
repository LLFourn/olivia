use crate::core::{Event, EventId, StampedOutcome};
use futures::channel::oneshot::Sender;
pub mod re_emitter;
pub mod redis;
pub mod time_ticker;
use futures::Stream;

pub struct Update<E> {
    pub update: E, // An Event or EventOutcome
    pub processed_notifier: Option<Sender<()>>,
}

impl<E> From<E> for Update<E> {
    fn from(update: E) -> Self {
        Self {
            update,
            processed_notifier: None,
        }
    }
}

impl From<EventId> for Update<Event> {
    fn from(id: EventId) -> Self {
        Update::from(Event::from(id))
    }
}

pub type EventStream = std::pin::Pin<Box<dyn Stream<Item = Update<Event>> + Send>>;
pub type OutcomeStream = std::pin::Pin<Box<dyn Stream<Item = Update<StampedOutcome>> + Send>>;
