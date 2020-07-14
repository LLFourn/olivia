use crate::core::{EventId, EventKind, VsMatchKind};
use chrono::NaiveDateTime;
use core::{convert::TryFrom, fmt};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct WireOutcome {
    pub event_id: EventId,
    pub outcome: String,
    pub time: NaiveDateTime,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(try_from = "WireOutcome")]
pub struct Outcome {
    pub event_id: EventId,
    pub outcome: ParsedOutcome,
    pub time: NaiveDateTime,
}

impl Outcome {
    pub fn completed_event_id(&self) -> String {
        format!("{}={}", self.event_id, self.outcome)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParsedOutcome {
    Occurred,
    Vs(VsOutcome),
    Win {
        winning_side: String,
        posited_won: bool,
    },
}

impl fmt::Display for ParsedOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ParsedOutcome::*;
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

#[derive(Debug, Error)]
pub enum OutcomeError {
    #[error("outcome for occur event was not ‘true’ got ‘{got}’")]
    OccurredNotTrue { got: String },
    #[error("entity ‘{entity}’ refers to something not part of the event")]
    InvalidEntity { entity: String },
    #[error("badly formatted outcome")]
    BadFormat,
}

impl TryFrom<WireOutcome> for Outcome {
    type Error = OutcomeError;

    fn try_from(outcome: WireOutcome) -> Result<Outcome, Self::Error> {
        let parsed_outcome =
            ParsedOutcome::try_from_id_and_outcome(&outcome.event_id, &outcome.outcome)?;
        Ok(Outcome {
            event_id: outcome.event_id,
            outcome: parsed_outcome,
            time: outcome.time,
        })
    }
}

impl ParsedOutcome {
    pub fn try_from_id_and_outcome(
        event_id: &EventId,
        outcome: &str,
    ) -> Result<ParsedOutcome, OutcomeError> {
        use ParsedOutcome::*;
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
