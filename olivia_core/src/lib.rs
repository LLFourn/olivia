#![no_std]
mod attestation;
mod schnorr;
mod event;
mod outcome;
pub use attestation::*;
pub use schnorr::*;
pub use event::*;
pub use outcome::*;

#[cfg_attr(not(feature = "std"), macro_use)]
extern crate alloc;

#[cfg(feature = "std")]
#[macro_use]
extern crate std;

use core::str::FromStr;

pub enum Entity {
    Event(event::Event),
    Outcome(outcome::EventOutcome),
}

impl FromStr for Entity {
    type Err = ParseEntityError;

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        match string.rfind("=") {
            Some(at) => {
                let event_id = EventId::from_str(&string[..at])?;
                if at != string.len() - 1 {
                    let outcome = Outcome::try_from_id_and_outcome(&event_id, &string[at + 1..])?;
                    Ok(Entity::Outcome(EventOutcome {
                        event_id,
                        outcome,
                        time: chrono::Utc::now().naive_utc(),
                    }))
                } else {
                    Err(ParseEntityError::Outcome(outcome::OutcomeError::BadFormat))
                }
            }
            None => Ok(Entity::Event(EventId::from_str(string)?.into())),
        }
    }
}

#[derive(Clone, Debug)]
pub enum ParseEntityError {
    Event(event::EventIdError),
    Outcome(outcome::OutcomeError),
}

impl core::fmt::Display for ParseEntityError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            ParseEntityError::Event(event_error) => write!(f, "Invalid event: {}", event_error),
            ParseEntityError::Outcome(outcome_error) => {
                write!(f, "Invalid outcome: {}", outcome_error)
            }
        }
    }
}


#[cfg(feature = "std")]
impl std::error::Error for ParseEntityError {}

impl From<outcome::OutcomeError> for ParseEntityError {
    fn from(e: outcome::OutcomeError) -> Self {
        ParseEntityError::Outcome(e)
    }
}

impl From<event::EventIdError> for ParseEntityError {
    fn from(e: event::EventIdError) -> Self {
        ParseEntityError::Event(e)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_entity() {
        match Entity::from_str("/foo/bar?occur").unwrap() {
            Entity::Event(event) => {
                assert_eq!(EventId::from_str("/foo/bar?occur").unwrap(), event.id)
            }
            _ => panic!(),
        }

        match Entity::from_str("/foo/bar?occur=true").unwrap() {
            Entity::Outcome(event_outcome) => {
                assert_eq!(
                    event_outcome.event_id,
                    EventId::from_str("/foo/bar?occur").unwrap()
                );
                assert_eq!(event_outcome.outcome, Outcome::Occurred);
            }
            _ => panic!(),
        }
    }
}
