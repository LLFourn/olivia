use crate::{EventId, EventKind, VsMatchKind};
use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use chrono::NaiveDateTime;
use core::{
    convert::{TryFrom, TryInto},
    fmt,
    str::FromStr,
};

#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct WireEventOutcome {
    #[serde(rename = "id")]
    pub event_id: EventId,
    pub outcome: String,
    pub time: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(try_from = "WireEventOutcome")]
pub struct StampedOutcome {
    pub outcome: Outcome,
    pub time: NaiveDateTime,
}

impl StampedOutcome {
    pub fn test_instance(event_id: &EventId) -> Self {
        Self {
            outcome: Outcome::test_instance(event_id),
            time: chrono::Utc::now().naive_utc(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub id: EventId,
    pub value: u64,
}

impl Outcome {
    pub fn test_instance(event_id: &EventId) -> Self {
        Outcome {
            id: event_id.clone(),
            value: event_id.n_outcomes() - 1,
        }
    }

    pub fn try_from_id_and_outcome(id: EventId, outcome: &str) -> Result<Self, OutcomeError> {
        let value = match id.event_kind() {
            EventKind::SingleOccurrence => {
                if outcome == "true" {
                    Ok(0)
                } else {
                    Err(OutcomeError::BadFormat)
                }
            }
            EventKind::VsMatch(kind) => {
                let (left, right) = id.parties().expect("it's a vs kind");
                match kind {
                    VsMatchKind::WinOrDraw => match outcome {
                        "draw" => Ok(2),
                        winner => match winner.strip_suffix("_win") {
                            Some(winner) => {
                                if winner == left {
                                    Ok(0)
                                } else if winner == right {
                                    Ok(1)
                                } else {
                                    Err(OutcomeError::InvalidEntity {
                                        entity: winner.to_string(),
                                    })
                                }
                            }
                            None => return Err(OutcomeError::BadFormat),
                        },
                    },
                    VsMatchKind::Win => {
                        if let Some(winner) = outcome.strip_suffix("_win") {
                            if winner == left {
                                Ok(0)
                            } else if winner == right {
                                Ok(1)
                            } else {
                                Err(OutcomeError::InvalidEntity {
                                    entity: winner.to_string(),
                                })
                            }
                        } else {
                            Err(OutcomeError::BadFormat)
                        }
                    }
                }
            }
            EventKind::Digits(n) => {
                let value = u64::from_str(outcome).or(Err(OutcomeError::BadFormat))?;
                if value.to_string().len() != n as usize {
                    return Err(OutcomeError::BadFormat);
                }
                Ok(value)
            }
        };

        Ok(Self { value: value?, id })
    }

    pub fn outcome_str(&self) -> String {
        let mut outcome_str = String::new();
        self.write_outcome_str(&mut outcome_str).unwrap();
        outcome_str
    }

    pub fn write_outcome_str(&self, f: &mut impl fmt::Write) -> fmt::Result {
        match (self.id.event_kind(), self.value) {
            (EventKind::SingleOccurrence, o) if o == Occur::Occurred as u64 => {
                write!(f, "{}", "true")
            }
            (EventKind::VsMatch(VsMatchKind::WinOrDraw), o) if o == WinOrDraw::LeftWon as u64 => {
                write!(f, "{}_win", self.id.parties().unwrap().0)
            }
            (EventKind::VsMatch(VsMatchKind::WinOrDraw), o) if o == WinOrDraw::RightWon as u64 => {
                write!(f, "{}_win", self.id.parties().unwrap().1)
            }
            (EventKind::VsMatch(VsMatchKind::WinOrDraw), o) if o == WinOrDraw::Draw as u64 => {
                write!(f, "draw")
            }
            (EventKind::VsMatch(VsMatchKind::Win), winner) if winner < 2 => match winner {
                0 => write!(f, "{}_win", self.id.parties().unwrap().0),
                1 => write!(f, "{}_win", self.id.parties().unwrap().1),
                _ => unreachable!("already checked < 2"),
            },
            (EventKind::Digits(..), value) => write!(f, "{}", value),
            _ => unreachable!("enum pairs must match if Outcome is valid"),
        }
    }

    pub fn attestation_indexes(&self) -> Vec<u32> {
        match self.id.event_kind() {
            EventKind::Digits(_n) => unimplemented!(),
            _ => vec![self.value.try_into().unwrap()],
        }
    }
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}=", self.id)?;
        self.write_outcome_str(f)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum OutcomeError {
    OccurredNotTrue { got: String },
    InvalidEntity { entity: String },
    BadFormat,
}

impl core::fmt::Display for OutcomeError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            OutcomeError::OccurredNotTrue { got } => {
                write!(f, "outcome for occur event was not ‘true’ got ‘{}’", got)
            }
            OutcomeError::InvalidEntity { entity } => write!(
                f,
                "entity ‘{}’ refers to something not part of the event",
                entity
            ),
            OutcomeError::BadFormat => write!(f, "badly formatted outcome"),
        }
    }
}

impl TryFrom<WireEventOutcome> for StampedOutcome {
    type Error = OutcomeError;

    fn try_from(outcome: WireEventOutcome) -> Result<Self, Self::Error> {
        let parsed_outcome = Outcome::try_from_id_and_outcome(outcome.event_id, &outcome.outcome)?;
        Ok(Self {
            outcome: parsed_outcome,
            time: outcome.time.unwrap_or(chrono::Utc::now().naive_utc()),
        })
    }
}

#[cfg(feature = "std")]
impl std::error::Error for OutcomeError {}

pub enum Win {
    PositedDidNotWin = 0,
    PositedWon = 1,
}

pub enum WinOrDraw {
    LeftWon = 0,
    RightWon = 1,
    Draw = 2,
}

pub enum Occur {
    Occurred = 0,
}
