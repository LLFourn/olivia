use crate::{EventId, EventKind, VsMatchKind};
use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use chrono::NaiveDateTime;
use core::{convert::TryFrom, fmt, str::FromStr};

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
    pub value: OutcomeValue,
}

impl Outcome {
    pub fn test_instance(event_id: &EventId) -> Self {
        Outcome {
            id: event_id.clone(),
            value: event_id.test_outcome(),
        }
    }

    pub fn fragments(&self) -> Vec<Fragment<'_>> {
        match self.id.event_kind() {
            EventKind::SingleOccurrence | EventKind::VsMatch(_) => {
                vec![Fragment::from_event_outcome(self, 0)]
            }
            EventKind::Digits(n) => (0..n)
                .map(|i| Fragment::from_event_outcome(self, i as usize))
                .collect(),
        }
    }

    pub fn try_from_id_and_outcome(id: EventId, outcome: &str) -> Result<Self, OutcomeError> {
        use OutcomeValue::*;
        use VsOutcome::*;
        let value = match id.event_kind() {
            EventKind::SingleOccurrence => {
                if outcome == "true" {
                    Ok(Occurred)
                } else {
                    Err(OutcomeError::BadFormat)
                }
            }
            EventKind::VsMatch(kind) => {
                let (left, right) = id.parties().expect("it's a vs kind");
                match kind {
                    VsMatchKind::WinOrDraw => match outcome {
                        "draw" => Ok(Vs(Draw)),
                        winner => match winner.strip_suffix("_win") {
                            Some(winner) => {
                                if winner == left {
                                    Ok(Vs(Winner(left.to_string())))
                                } else if winner == right {
                                    Ok(Vs(Winner(right.to_string())))
                                } else {
                                    Err(OutcomeError::InvalidEntity {
                                        entity: winner.to_string(),
                                    })
                                }
                            }
                            None => return Err(OutcomeError::BadFormat),
                        },
                    },
                    VsMatchKind::Win {
                        right_posited_to_win,
                    } => {
                        let (posited_to_win, other) = if right_posited_to_win {
                            (right, left)
                        } else {
                            (left, right)
                        };

                        if let Some(winner) = outcome.strip_suffix("_win") {
                            if winner == posited_to_win {
                                Ok(Win {
                                    winning_side: winner.to_string(),
                                    posited_won: true,
                                })
                            } else {
                                Err(OutcomeError::InvalidEntity {
                                    entity: winner.to_string(),
                                })
                            }
                        } else if let Some(win_or_draw) = outcome.strip_suffix("_win") {
                            if win_or_draw == other {
                                Ok(Win {
                                    winning_side: win_or_draw.to_string(),
                                    posited_won: false,
                                })
                            } else {
                                Err(OutcomeError::InvalidEntity {
                                    entity: win_or_draw.to_string(),
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

                Ok(Digits(value))
            }
        };

        Ok(Self { value: value?, id })
    }
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}={}", self.id, self.value)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum OutcomeValue {
    Occurred,
    Vs(VsOutcome),
    Win {
        winning_side: String,
        posited_won: bool,
    },
    Digits(u64),
}

impl OutcomeValue {
    pub fn write_to(&self, t: &mut impl fmt::Write) -> fmt::Result {
        use OutcomeValue::*;
        use VsOutcome::*;
        match self {
            Occurred => write!(t, "{}", "true"),
            Vs(Winner(winner)) => write!(t, "{}_win", winner),
            Vs(Draw) => write!(t, "draw"),
            Win {
                winning_side,
                posited_won,
            } => {
                if *posited_won {
                    write!(t, "{}_win", winning_side)
                } else {
                    write!(t, "{}_win", winning_side)
                }
            }
            Digits(value) => write!(t, "{}", value),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fragment<'a> {
    pub index: usize,
    pub outcome: &'a Outcome,
}

impl Fragment<'_> {
    pub fn from_event_outcome(outcome: &Outcome, index: usize) -> Fragment<'_> {
        Fragment { index, outcome }
    }

    pub fn attestation_string(&self) -> String {
        format!("{}.{}={}", self.outcome.id, self.index, self.outcome.value)
    }

    pub fn write_to(&self, f: &mut impl fmt::Write) -> fmt::Result {
        use OutcomeValue::*;
        match self.outcome.value {
            Occurred | Vs(_) | Win { .. } => self.outcome.value.write_to(f),
            Digits(value) => write!(f, "{}", value.to_string().chars().nth(self.index).unwrap()),
        }
    }
}

impl fmt::Display for Fragment<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_to(f)
    }
}

impl fmt::Display for OutcomeValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_to(f)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum VsOutcome {
    Winner(String),
    Draw,
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

impl OutcomeValue {}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_digits_integer() {
        let event_id = EventId::from_str("/foo/bar?digits_6").unwrap();
        let outcome = Outcome::try_from_id_and_outcome(event_id, "123456").unwrap();
        if let OutcomeValue::Digits(value) = outcome.value {
            assert_eq!(value, 123456);
        } else {
            panic!("wrong outcome kind");
        }
    }
}
