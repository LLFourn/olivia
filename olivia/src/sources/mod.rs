use olivia_core::{Event, EventId, PathRef, PrefixPath};
use tokio::sync::oneshot::Sender;
use tokio_stream as stream;
pub mod complete_related;
pub mod predicate;
pub mod redis;
pub mod ticker;
#[cfg(test)]
mod time_tests;

pub struct Update<E> {
    pub update: E, // An Event or EventOutcome
    pub processed_notifier: Option<Sender<bool>>,
}

impl<E> From<E> for Update<E> {
    fn from(update: E) -> Self {
        Self {
            update,
            processed_notifier: None,
        }
    }
}

impl<E> Update<E> {
    pub fn new(e: E) -> Self {
        Self {
            update: e,
            processed_notifier: None,
        }
    }
}

impl From<EventId> for Update<Event> {
    fn from(id: EventId) -> Self {
        Update::from(Event::from(id))
    }
}

impl<E: PrefixPath> PrefixPath for Update<E> {
    fn prefix_path(mut self, path: PathRef<'_>) -> Self {
        self.update = self.update.prefix_path(path);
        self
    }

    fn strip_prefix_path(mut self, path: PathRef<'_>) -> Self {
        self.update = self.update.strip_prefix_path(path);
        self
    }
}

pub type Stream<T> = std::pin::Pin<Box<dyn stream::Stream<Item = Update<T>> + Send>>;
