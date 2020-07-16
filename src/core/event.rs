use crate::curve::{
    ed25519::{self, Ed25519},
    secp256k1::{self, Secp256k1},
    Curve,
};
use chrono::NaiveDateTime;
use core::fmt;
use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub enum EventKind {
    VsMatch(VsMatchKind),
    SingleOccurrence,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VsMatchKind {
    WinOrDraw,
    Win { right_posited_to_win: bool },
}

impl fmt::Display for EventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                EventKind::VsMatch(kind) => {
                    match kind {
                        VsMatchKind::Win {
                            right_posited_to_win,
                        } => match right_posited_to_win {
                            true => "right-win",
                            false => "left-win",
                        },
                        VsMatchKind::WinOrDraw => "vs",
                    }
                }
                EventKind::SingleOccurrence => "occur",
            }
        )
    }
}

lazy_static! {
    static ref EVENT_ID_RE: regex::Regex = regex::Regex::new(
        r"^(?P<path>[a-zA-Z][0-9a-zA-Z:_-]*(?:/[0-9A-Za-z:_-]+)+)\.(?P<event_kind>[a-z0-9-]+)$"
    )
    .unwrap();
    static ref VS_RE: regex::Regex =
        regex::Regex::new(r"^([a-zA-Z0-9:-]+)_([a-zA-z0-9:-]+)$").unwrap();
}

#[derive(
    Clone, Debug, PartialEq, Hash, Eq, FromSqlRow, AsExpression, Serialize, PartialOrd, Ord,
)]
#[sql_type = "diesel::sql_types::Text"]
pub struct EventId(pub(crate) String);

impl EventId {
    pub fn as_bytes(&self) -> &[u8] {
        self.as_str().as_bytes()
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn as_path(&self) -> PathRef<'_> {
        PathRef(self.0.as_str())
    }

    pub fn node(&self) -> PathRef<'_> {
        let s = self.0.as_str();
        let dot_index = s.rfind('.').unwrap();
        PathRef(&s[..dot_index])
    }

    pub fn parties(&self) -> Option<(&str, &str)> {
        if let EventKind::VsMatch(_) = self.event_kind() {
            let mut parties = self.node().last().split('_');
            Some((parties.next().unwrap(), parties.next().unwrap()))
        } else {
            None
        }
    }

    pub fn event_kind(&self) -> EventKind {
        let last = self.as_path().last();
        let index = last.find('.').unwrap();
        let event_kind = &last[index + 1..];
        match event_kind {
            "vs" | "left-win" | "right-win" => {
                let vs_kind = match event_kind {
                    "vs" => VsMatchKind::WinOrDraw,
                    "left-win" => VsMatchKind::Win {
                        right_posited_to_win: false,
                    },
                    "right-win" => VsMatchKind::Win {
                        right_posited_to_win: true,
                    },
                    _ => unreachable!("we have narrowed this aready"),
                };
                EventKind::VsMatch(vs_kind)
            }
            "occur" => EventKind::SingleOccurrence,
            this => unreachable!(
                "valid event ids have already been checked to not be {}",
                this
            ),
        }
    }

    pub fn replace_kind(&self, kind: EventKind) -> EventId {
        EventId(format!("{}.{}", self.node(), kind))
    }
}

#[derive(Debug, Error)]
pub enum EventIdError {
    #[error("invalid event id format")]
    BadFormat,
    #[error("unknown event kind {0}")]
    UnknownEventKind(String),
}

impl FromStr for EventId {
    type Err = EventIdError;

    fn from_str(string: &str) -> Result<EventId, Self::Err> {
        match EVENT_ID_RE.captures(&string) {
            Some(captures) => {
                let path = PathRef::from(captures.name("path").unwrap().as_str());
                let event_kind = captures.name("event_kind").unwrap().as_str();
                let valid_kind = match event_kind {
                    "vs" | "left-win" | "right-win" => match VS_RE.captures(path.last()) {
                        Some(capture) => {
                            capture.get(0).unwrap().as_str() != capture.get(1).unwrap().as_str()
                        }
                        _ => return Err(EventIdError::BadFormat),
                    },
                    "occur" => true,
                    _ => false,
                };

                if !valid_kind {
                    return Err(EventIdError::UnknownEventKind(event_kind.into()));
                }

                Ok(EventId(string.to_string()))
            }
            None => Err(EventIdError::BadFormat),
        }
    }
}

impl From<EventId> for String {
    fn from(eid: EventId) -> Self {
        eid.0
    }
}

impl From<EventId> for Event {
    fn from(id: EventId) -> Self {
        Self {
            id,
            expected_outcome_time: None,
        }
    }
}

impl PartialEq<str> for EventId {
    fn eq(&self, rhs: &str) -> bool {
        self.0 == rhs
    }
}

// rust made me do it
impl PartialEq<&str> for EventId {
    fn eq(&self, rhs: &&str) -> bool {
        self.0 == *rhs
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathRef<'a>(&'a str);

impl<'a> PathRef<'a> {
    pub fn parent(self) -> Option<PathRef<'a>> {
        self.0.rfind('/').map(|at| PathRef(&self.0[..at]))
    }

    pub fn first(self) -> &'a str {
        self.0.find('/').map(|at| &self.0[0..at]).unwrap_or(self.0)
    }

    pub fn last(self) -> &'a str {
        self.0
            .rfind('/')
            .map(|at| &self.0[at + 1..])
            .unwrap_or(&self.0[..])
    }

    pub fn as_str(self) -> &'a str {
        self.0
    }

    pub fn root() -> Self {
        PathRef("")
    }

    pub fn is_root(self) -> bool {
        self == Self::root()
    }
}

impl<'a> From<&'a str> for PathRef<'a> {
    fn from(s: &'a str) -> Self {
        PathRef(s)
    }
}

impl fmt::Display for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl fmt::Display for PathRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub id: EventId,
    pub expected_outcome_time: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Nonce {
    pub ed25519: ed25519::PublicKey,
    pub secp256k1: secp256k1::PublicKey,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ObservedEvent {
    pub event: Event,
    pub nonce: Nonce,
    pub attestation: Option<crate::core::Attestation>,
}

impl ObservedEvent {
    pub fn signatures(&self) -> Option<Signatures> {
        self.attestation.clone().map(|attestation| {
            let scalars = attestation.scalars;
            Signatures {
                secp256k1: Secp256k1::signature_from_scalar_and_nonce(
                    scalars.secp256k1,
                    self.nonce.secp256k1.clone(),
                ),
                ed25519: Ed25519::signature_from_scalar_and_nonce(
                    scalars.ed25519,
                    self.nonce.ed25519.clone(),
                ),
            }
        })
    }
}

pub struct Signatures {
    pub ed25519: <ed25519::Ed25519 as Curve>::SchnorrSignature,
    pub secp256k1: <secp256k1::Secp256k1 as Curve>::SchnorrSignature,
}

mod sql_impls {
    use super::*;
    use diesel::{
        backend::Backend,
        deserialize::{self, *},
        serialize::{self, *},
        sql_types,
    };
    use std::io::Write;

    impl<DB: Backend> FromSql<sql_types::Text, DB> for EventId
    where
        String: FromSql<sql_types::Text, DB>,
    {
        fn from_sql(bytes: Option<&DB::RawValue>) -> deserialize::Result<Self> {
            let string = <String as FromSql<sql_types::Text, DB>>::from_sql(bytes)?;
            Ok(EventId(string))
        }
    }

    impl<DB: Backend> ToSql<sql_types::Text, DB> for EventId {
        fn to_sql<W: Write>(&self, out: &mut Output<W, DB>) -> serialize::Result {
            ToSql::<sql_types::Text, DB>::to_sql(self.as_str(), out)
        }
    }
}

mod serde_impl {
    use super::*;
    use core::fmt;
    use serde::de;
    impl<'de> de::Deserialize<'de> for EventId {
        fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<EventId, D::Error> {
            struct Visitor;

            impl<'de> de::Visitor<'de> for Visitor {
                type Value = EventId;

                fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                    formatter.write_str("A valid event_id")
                }

                fn visit_str<E: de::Error>(self, v: &str) -> Result<EventId, E> {
                    EventId::from_str(v).map_err(|e| E::custom(format!("{}", e)))
                }
            }

            deserializer.deserialize_any(Visitor)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use crate::core::{Outcome, VsOutcome};
    impl EventId {
        pub fn default_outcome(&self) -> Outcome {
            use Outcome::*;

            match self.event_kind() {
                EventKind::VsMatch(kind) => {
                    let (left, right) = self.parties().unwrap();
                    use VsOutcome::*;
                    match kind {
                        VsMatchKind::WinOrDraw => Vs(Winner(left.to_string())),
                        VsMatchKind::Win {
                            right_posited_to_win,
                        } => Win {
                            winning_side: right.to_string(),
                            posited_won: right_posited_to_win == true,
                        },
                    }
                }
                EventKind::SingleOccurrence => Outcome::Occurred,
            }
        }
    }

    #[test]
    fn event_id_from_str() {
        assert!(EventId::from_str("/foo/bar.occur").is_err());
        assert!(EventId::from_str("foo/bar.occur/").is_err());
        assert!(EventId::from_str("foo.occur").is_err());
        assert!(EventId::from_str("foo.occur").is_err());
        assert!(EventId::from_str("foo/bar.occur").is_ok());
        assert!(EventId::from_str("foo/bar/baz.occur").is_ok());
        assert!(EventId::from_str("foo/23/52.occur").is_ok());
        assert!(EventId::from_str("foo/bar/FOO_BAR.vs").is_ok());
        assert!(EventId::from_str("foo/bar/FOO-BAR.vs").is_err());
    }

    #[test]
    fn event_id_parent() {
        let event_id = EventId::from_str("one/two/three.occur").unwrap();
        assert_eq!(event_id.node().as_str(), "one/two/three");
        assert_eq!(event_id.node().parent().unwrap().as_str(), "one/two");
        assert_eq!(
            event_id.node().parent().unwrap().parent().unwrap().as_str(),
            "one",
        );
        assert_eq!(
            event_id.node().parent().unwrap().parent().unwrap().parent(),
            None
        );
    }
}
