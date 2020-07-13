use crate::curve::{ed25519, secp256k1};
use chrono::NaiveDateTime;
use core::fmt;
use serde::de;
use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub enum EventKind {
    VsMatch { kind: VsMatchKind },
    SingleOccurrence,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VsMatchKind {
    Draw,
    Win { right: bool },
}

impl fmt::Display for EventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                EventKind::VsMatch { kind } => {
                    match kind {
                        VsMatchKind::Win { right } => match right {
                            true => "right-win",
                            false => "left-win",
                        },
                        VsMatchKind::Draw => "vs",
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

#[derive(Clone, Debug, PartialEq, Hash, Eq, FromSqlRow, AsExpression, Serialize)]
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

    pub fn event_kind(&self) -> EventKind {
        let slug = self.as_path().slug();
        let index = slug.find('.').unwrap();
        let event_kind = &slug[index + 1..];
        match event_kind {
            "vs" => EventKind::VsMatch {
                kind: VsMatchKind::Draw,
            },
            "left-win" => EventKind::VsMatch {
                kind: VsMatchKind::Win { right: false },
            },
            "right-win" => EventKind::VsMatch {
                kind: VsMatchKind::Win { right: true },
            },
            "occur" => EventKind::SingleOccurrence,
            this => unreachable!(
                "valid event ids have already been checked to not be {}",
                this
            ),
        }
    }

    pub fn outcomes(&self) -> Vec<String> {
        match self.event_kind() {
            EventKind::VsMatch { kind } => {
                let slug = self.node().slug();
                let mut teams = slug.split('_');
                let one = teams.next().unwrap();
                let two = teams.next().unwrap();
                match kind {
                    VsMatchKind::Draw => vec![
                        format!("{}-win", one),
                        format!("{}-win", two),
                        "draw".to_string(),
                    ],
                    VsMatchKind::Win { .. } => vec!["true".to_string(), "false".to_string()],
                }
            }
            EventKind::SingleOccurrence => vec!["true".to_string()],
        }
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
                    "vs" | "left-win" | "right-win" => VS_RE.is_match(path.slug()),
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

#[derive(Clone, Debug, PartialEq, Hash, Eq)]
pub struct Path(pub(crate) String);

impl Path {
    pub fn as_ref(&self) -> PathRef<'_> {
        PathRef(self.0.as_str())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn root() -> Self {
        Path("".to_string())
    }
}

impl PartialEq<&str> for Path {
    fn eq(&self, rhs: &&str) -> bool {
        self.0 == *rhs
    }
}

impl PartialEq<Path> for &str {
    fn eq(&self, rhs: &Path) -> bool {
        *self == rhs.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathRef<'a>(&'a str);

impl<'a> PathRef<'a> {
    pub fn parent(self) -> Option<PathRef<'a>> {
        self.0.rfind('/').map(|at| PathRef(&self.0[..at]))
    }

    pub fn slug(self) -> &'a str {
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

impl From<PathRef<'_>> for Path {
    fn from(path: PathRef<'_>) -> Self {
        Self(path.0.to_string())
    }
}

impl<'a> From<&'a str> for PathRef<'a> {
    fn from(s: &'a str) -> Self {
        PathRef(s)
    }
}

impl From<String> for Path {
    fn from(from: String) -> Self {
        Path(from)
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub expected_outcome_time: Option<NaiveDateTime>,
}

impl Event {
    pub fn outcomes(&self) -> Vec<String> {
        self.id.outcomes()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Outcome {
    pub event_id: EventId,
    pub outcome: String,
    pub time: NaiveDateTime,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Attestation {
    pub outcome: String,
    pub time: NaiveDateTime,
    pub scalars: Scalars,
}

impl Attestation {
    pub fn new(outcome: String, mut time: NaiveDateTime, scalars: Scalars) -> Self {
        use chrono::Timelike;
        time = time.with_nanosecond(0).expect("0 is valid");
        Attestation {
            outcome,
            time,
            scalars,
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Scalars {
    pub ed25519: ed25519::SchnorrScalar,
    pub secp256k1: secp256k1::SchnorrScalar,
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
    pub attestation: Option<Attestation>,
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

#[cfg(test)]
mod test {
    use super::*;

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
    fn event_id_outcomes() {
        assert_eq!(
            EventId::from_str("foo/bar/FOO_BAR.vs").unwrap().outcomes(),
            ["FOO-win", "BAR-win", "draw"]
        );

        assert_eq!(
            EventId::from_str("foo/bar/FOO_BAR.left-win")
                .unwrap()
                .outcomes(),
            ["true", "false"]
        );

        assert_eq!(
            EventId::from_str("foo/bar.occur").unwrap().outcomes(),
            ["true"]
        );
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
