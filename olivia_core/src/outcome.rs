use crate::{EventId, EventKind, VsMatchKind};
use alloc::string::{String, ToString};
use chrono::NaiveDateTime;
use core::{convert::TryFrom, fmt};

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
pub struct EventOutcome {
    pub event_id: EventId,
    pub outcome: Outcome,
    pub time: NaiveDateTime,
}

impl EventOutcome {
    pub fn attestation_string(&self) -> String {
        format!("{}={}", self.event_id, self.outcome)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Occurred,
    Vs(VsOutcome),
    Win {
        winning_side: String,
        posited_won: bool,
    },
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Outcome::*;
        use VsOutcome::*;
        write!(
            f,
            "{}",
            match self {
                Occurred => "true".to_string(),
                Vs(Winner(winner)) => format!("{}_win", winner),
                Vs(Draw) => "draw".to_string(),
                Win {
                    winning_side,
                    posited_won,
                } => {
                    if *posited_won {
                        format!("{}_win", winning_side)
                    } else {
                        format!("{}_win-or-draw", winning_side)
                    }
                }
            }
        )
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

impl TryFrom<WireEventOutcome> for EventOutcome {
    type Error = OutcomeError;

    fn try_from(outcome: WireEventOutcome) -> Result<Self, Self::Error> {
        let parsed_outcome = Outcome::try_from_id_and_outcome(&outcome.event_id, &outcome.outcome)?;
        Ok(Self {
            event_id: outcome.event_id,
            outcome: parsed_outcome,
            time: outcome.time.unwrap_or(chrono::Utc::now().naive_utc()),
        })
    }
}

impl Outcome {
    pub fn try_from_id_and_outcome(
        event_id: &EventId,
        outcome: &str,
    ) -> Result<Self, OutcomeError> {
        use Outcome::*;
        use VsOutcome::*;
        match event_id.event_kind() {
            EventKind::SingleOccurrence => {
                if outcome == "true" {
                    Ok(Occurred)
                } else {
                    Err(OutcomeError::BadFormat)
                }
            }
            EventKind::VsMatch(kind) => {
                let (left, right) = event_id.parties().expect("it's a vs kind");
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
                        } else if let Some(win_or_draw) = outcome.strip_suffix("_win-or-draw") {
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
        }
    }
}
