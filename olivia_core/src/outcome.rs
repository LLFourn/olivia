use crate::{EventId, EventKind, PrefixPath, VsMatchKind};
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
                    0
                } else {
                    return Err(OutcomeError::Invalid {
                        outcome: outcome.to_string(),
                    });
                }
            }
            EventKind::VsMatch(kind) => {
                let (left, right) = id.parties().expect("it's a vs kind");
                match kind {
                    VsMatchKind::WinOrDraw => match outcome {
                        "draw" => WinOrDraw::Draw as u64,
                        winner => match winner.strip_suffix("_win") {
                            Some(winner) => {
                                if winner == left {
                                    WinOrDraw::Left as u64
                                } else if winner == right {
                                    WinOrDraw::Right as u64
                                } else {
                                    return Err(OutcomeError::Invalid {
                                        outcome: winner.to_string(),
                                    });
                                }
                            }
                            None => {
                                return Err(OutcomeError::Invalid {
                                    outcome: outcome.to_string(),
                                })
                            }
                        },
                    },
                    VsMatchKind::Win => {
                        let winner = outcome;
                        if winner == left {
                            Win::Left as u64
                        } else if winner == right {
                            Win::Right as u64
                        } else {
                            return Err(OutcomeError::Invalid {
                                outcome: winner.to_string(),
                            });
                        }
                    }
                }
            }
            EventKind::Predicate { .. } => {
                bool::from_str(outcome).map_err(|_| OutcomeError::Invalid {
                    outcome: outcome.to_string(),
                })? as u64
            }
            EventKind::Price { .. } => {
                u64::from_str(outcome).map_err(|_| OutcomeError::Invalid {
                    outcome: outcome.into(),
                })?
            }
        };

        Ok(Self { value, id })
    }

    pub fn outcome_string(&self) -> String {
        let mut outcome_str = String::new();
        self.write_outcome_string(&mut outcome_str).unwrap();
        outcome_str
    }

    pub fn write_outcome_string(&self, f: &mut impl fmt::Write) -> fmt::Result {
        match (self.id.event_kind(), self.value) {
            (EventKind::SingleOccurrence, o) => {
                match Occur::try_from(o).expect("outcome value should be less than 1") {
                    Occur::Occurred => write!(f, "{}", "true"),
                }
            }
            (EventKind::VsMatch(VsMatchKind::WinOrDraw), o) => {
                match WinOrDraw::try_from(o).expect("outcome value should be less than 3") {
                    WinOrDraw::Left => write!(f, "{}_win", self.id.parties().unwrap().0),
                    WinOrDraw::Right => write!(f, "{}_win", self.id.parties().unwrap().1),
                    WinOrDraw::Draw => write!(f, "draw"),
                }
            }
            (EventKind::VsMatch(VsMatchKind::Win), winner) => {
                match Win::try_from(winner).expect("outcome should be less than 2") {
                    Win::Left => write!(f, "{}", self.id.parties().unwrap().0),
                    Win::Right => write!(f, "{}", self.id.parties().unwrap().1),
                }
            }
            (EventKind::Predicate { .. }, truth) => {
                assert!(truth < 2);
                write!(f, "{}", truth != 0)
            }
            (EventKind::Price { .. }, price) => write!(f, "{}", price),
        }
    }

    pub fn attestation_indexes(&self) -> Vec<u32> {
        match self.id.event_kind() {
            EventKind::Price { n_digits } => {
                let cap = u64::MAX.checked_shr(64 - n_digits as u32).unwrap_or(0);
                let value = self.value.min(cap);
                (0..n_digits)
                    .map(|i| (value & (1 << i) != 0) as u32)
                    .rev()
                    .collect()
            }
            _ => vec![self.value.try_into().unwrap()],
        }
    }

    pub fn attestation_string(&self) -> Vec<u8> {
        let mut att_string = self.id.as_bytes().to_vec();
        att_string.push('!' as u8);
        att_string.append(&mut self.value.to_be_bytes().to_vec());
        att_string
    }
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:", self.id)?;
        self.write_outcome_string(f)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum OutcomeError {
    Invalid { outcome: String },
}

impl core::fmt::Display for OutcomeError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            OutcomeError::Invalid { outcome: entity } => write!(
                f,
                "entity ‘{}’ refers to something not part of the event",
                entity
            ),
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

impl std::error::Error for OutcomeError {}

impl PrefixPath for Outcome {
    fn prefix_path(mut self, path: crate::PathRef<'_>) -> Self {
        self.id = self.id.prefix_path(path);
        self
    }

    fn strip_prefix_path(mut self, path: crate::PathRef<'_>) -> Self {
        self.id = self.id.strip_prefix_path(path);
        self
    }
}

impl PrefixPath for StampedOutcome {
    fn prefix_path(mut self, path: crate::PathRef<'_>) -> Self {
        self.outcome = self.outcome.prefix_path(path);
        self
    }

    fn strip_prefix_path(mut self, path: crate::PathRef<'_>) -> Self {
        self.outcome = self.outcome.strip_prefix_path(path);
        self
    }
}

// Oh ffs I shouldn't have to do this myself
macro_rules! enum_try_from_int {
    (
        #[repr($T: ident)]
        $( #[$meta: meta] )*
        $vis: vis enum $Name: ident {
            $(
                $Variant: ident = $value: expr
            ),*
            $( , )?
        }
    ) => {
        #[repr($T)]
        $( #[$meta] )*
        $vis enum $Name {
            $(
                $Variant = $value
            ),*
        }

        impl core::convert::TryFrom<$T> for $Name {
            type Error = ();

            fn try_from(value: $T) -> Result<$Name, ()> {
                match value {
                    $(
                        $value => Ok($Name::$Variant),
                    )*
                    _ => Err(())
                }
            }
        }
    }
}

enum_try_from_int! {
    #[repr(u64)]
    pub enum Win {
        Left = 0,
        Right = 1,
    }
}

enum_try_from_int! {
    #[repr(u64)]
    pub enum WinOrDraw {
        Left = 0,
        Right = 1,
        Draw = 2,
    }
}

enum_try_from_int! {
    #[repr(u64)]
    pub enum Occur {
        Occurred = 0,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn price_attestation_indexes() {
        let outcome = Outcome {
            id: EventId::from_str("/foo/bar.price?n=6").unwrap(),
            value: 0b100100,
        };

        assert_eq!(outcome.attestation_indexes(), vec![1, 0, 0, 1, 0, 0]);
    }

    #[test]
    fn overflow_attestation_indexes() {
        let outcome = Outcome {
            id: EventId::from_str("/foo/bar.price?n=6").unwrap(),
            value: 0b1100100,
        };

        assert_eq!(outcome.attestation_indexes(), vec![1, 1, 1, 1, 1, 1]);
    }

    #[test]
    fn edge_cases_attestation_indexes() {
        let outcome = Outcome {
            id: EventId::from_str("/foo/bar.price?n=64").unwrap(),
            value: u64::MAX - 1,
        };

        assert_eq!(
            outcome.attestation_indexes(),
            vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 1, 1, 0
            ]
        );

        let outcome = Outcome {
            id: EventId::from_str("/foo/bar.price").unwrap(),
            value: u64::MAX - 1,
        };

        assert_eq!(outcome.attestation_indexes(), vec![0u32; 0]);
    }
}
