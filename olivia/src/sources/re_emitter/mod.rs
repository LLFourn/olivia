mod vs;
pub use vs::*;
mod random;
pub use random::*;

use crate::sources::{EventStream, OutcomeStream};

pub trait EventReEmitter {
    fn re_emit_events(&self, events: EventStream) -> EventStream;
}

pub trait OutcomeReEmitter {
    fn re_emit_outcomes(&self, outcomes: OutcomeStream) -> OutcomeStream;
}
