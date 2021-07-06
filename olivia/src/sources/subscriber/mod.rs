// mod vs;
// pub use vs::*;
mod random;

use crate::sources;
pub use random::*;

// this abstraction is not necessary!
pub type Stream<T> = std::pin::Pin<Box<dyn futures::Stream<Item = T> + Send>>;

pub trait Subscriber<T> {
    fn start(&self, events: Stream<T>) -> sources::Stream<T>;
}
