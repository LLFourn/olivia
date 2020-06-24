use futures::channel::oneshot::Sender;
pub mod redis;
pub mod time_ticker;

pub struct Update<E> {
    pub update: E, // An Event or Outcome
    pub processed_notifier: Option<Sender<()>>,
}

pub struct EventSourceLog {}
