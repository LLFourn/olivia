use crate::curve::{ed25519, secp256k1};
use chrono::NaiveDateTime;
use diesel::sql_types::Jsonb;
use serde::de;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Party {
    id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, AsExpression, FromSqlRow)]
#[sql_type = "Jsonb"]
#[serde(tag = "type")]
#[serde(rename_all = "kebab-case")]
pub enum EventKind {
    VsMatch { one: Party, two: Party },
    SingleOccurrence,
    CoinToss { n: u32 },
}

lazy_static! {
    static ref EVENT_ID_RE: regex::Regex =
        regex::Regex::new(r"^[a-zA-Z][0-9a-zA-Z-]*(/[0-9A-Za-z-]+)+").unwrap();
}

#[derive(Clone, Debug, Serialize, PartialEq, Hash, Eq)]
pub struct EventId(String);

#[derive(Clone, Debug, PartialEq)]
pub struct Path(String);

impl Path {
    pub fn as_ref(&self) -> PathRef<'_> {
        PathRef(self.0.as_str())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn parent(&self) -> Option<PathRef<'_>> {
        self.as_ref().parent()
    }

    pub fn is_root(&self) -> bool {
        self.0.is_empty()
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

    pub fn as_str(self) -> &'a str {
        self.0
    }

    pub fn root() -> Self {
        PathRef("")
    }

    pub fn is_root(&self) -> bool {
        *self == Self::root()
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

impl From<String> for EventId {
    fn from(from: String) -> Self {
        EventId(from)
    }
}

impl From<Path> for EventId {
    fn from(path: Path) -> Self {
        EventId::from(path.0)
    }
}

impl From<EventId> for String {
    fn from(id: EventId) -> Self {
        id.0
    }
}

impl AsRef<str> for EventId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl EventId {
    pub fn parent(&self) -> PathRef<'_> {
        self.as_path().parent().unwrap()
    }
    pub fn as_path(&self) -> PathRef<'_> {
        PathRef(&self.0)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
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
                if EVENT_ID_RE.is_match(v) {
                    Ok(EventId(v.to_string()))
                } else {
                    Err(E::custom(format!("'{}' is not a valid event_id", v)))
                }
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub kind: EventKind,
    pub expected_outcome_time: NaiveDateTime,
}

impl Event {
    pub fn outcomes(&self) -> Vec<String> {
        use EventKind::*;
        match self.kind {
            VsMatch { ref one, ref two } => {
                vec![format!("{}-WIN", one.id), format!("{}-WIN", two.id)]
            }
            SingleOccurrence => vec!["OCCURRED".to_string()],
            CoinToss { n } => (0..n).into_iter().map(|x| x.to_string()).collect(),
        }
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
        deserialize::{self, FromSql},
        pg::Pg,
        serialize::{self, Output, ToSql},
        sql_types,
    };
    use std::io::prelude::*;

    impl ToSql<sql_types::Jsonb, Pg> for EventKind {
        fn to_sql<W: Write>(&self, out: &mut Output<W, Pg>) -> serialize::Result {
            let json_value = &serde_json::to_value(self)?;
            ToSql::<sql_types::Jsonb, Pg>::to_sql(json_value, out)
        }
    }

    impl FromSql<sql_types::Jsonb, Pg> for EventKind {
        fn from_sql(bytes: Option<&[u8]>) -> deserialize::Result<Self> {
            let json_value = FromSql::<sql_types::Jsonb, Pg>::from_sql(bytes)?;
            serde_json::value::from_value::<EventKind>(json_value).map_err(Into::into)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn event_id_deserialization() {
        assert!(serde_json::from_str::<EventId>(r#""/foo/bar""#).is_err());
        assert!(serde_json::from_str::<EventId>(r#""/foo/bar/""#).is_err());
        assert!(serde_json::from_str::<EventId>(r#""foo/""#).is_err());
        assert!(serde_json::from_str::<EventId>(r#""foo""#).is_err());

        assert!(serde_json::from_str::<EventId>(r#""foo/bar""#).is_ok());
        assert!(serde_json::from_str::<EventId>(r#""foo/bar/baz52""#).is_ok());
        assert!(serde_json::from_str::<EventId>(r#""foo/23/52""#).is_ok());
    }

    #[test]
    fn event_id_parent() {
        let event_id = EventId::from("one/two/three".to_string());
        assert_eq!(event_id.as_path().as_str(), "one/two/three");
        assert_eq!(event_id.parent(), event_id.as_path().parent().unwrap());
        assert_eq!(event_id.as_path().parent().unwrap().as_str(), "one/two");
        assert_eq!(
            event_id
                .as_path()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .as_str(),
            "one",
        );
        assert_eq!(
            event_id
                .as_path()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .parent(),
            None
        );
    }
}
