use crate::{Event, EventId, EventIdError, Outcome, OutcomeError};
use core::str::FromStr;

pub enum Entity {
    Event(Event),
    Outcome(Outcome),
}

impl FromStr for Entity {
    type Err = ParseEntityError;

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        match string.rfind("=") {
            Some(at) => {
                let event_id = EventId::from_str(&string[..at])?;
                if at != string.len() - 1 {
                    let outcome = Outcome::try_from_id_and_outcome(event_id, &string[at + 1..])?;
                    Ok(Entity::Outcome(outcome))
                } else {
                    Err(ParseEntityError::Outcome(OutcomeError::BadFormat))
                }
            }
            None => Ok(Entity::Event(EventId::from_str(string)?.into())),
        }
    }
}

#[derive(Clone, Debug)]
pub enum ParseEntityError {
    Event(EventIdError),
    Outcome(OutcomeError),
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

impl From<OutcomeError> for ParseEntityError {
    fn from(e: OutcomeError) -> Self {
        ParseEntityError::Outcome(e)
    }
}

impl From<EventIdError> for ParseEntityError {
    fn from(e: EventIdError) -> Self {
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
            Entity::Outcome(outcome) => {
                assert_eq!(outcome.id, EventId::from_str("/foo/bar?occur").unwrap());
                assert_eq!(outcome.value, crate::Occur::Occurred as u64);
            }
            _ => panic!(),
        }
    }
}
