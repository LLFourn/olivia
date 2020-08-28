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
    static ref VS_RE: regex::Regex =
        regex::Regex::new(r"^([a-zA-Z0-9:-]+)_([a-zA-z0-9:-]+)$").unwrap();
}

#[derive(Clone, Debug, PartialEq, Hash, Eq, FromSqlRow, AsExpression, PartialOrd, Ord)]
#[sql_type = "diesel::sql_types::Text"]
pub struct EventId(url::Url);

impl EventId {
    pub fn as_bytes(&self) -> &[u8] {
        self.as_str().as_bytes()
    }

    pub fn as_str(&self) -> &str {
        let scheme_pos = self.0.as_str().find(':').expect("there is always a scheme");
        &self.0.as_str()[scheme_pos + 1..]
    }

    pub fn as_path(&self) -> PathRef<'_> {
        PathRef(self.0.path())
    }

    pub fn parties(&self) -> Option<(&str, &str)> {
        if let EventKind::VsMatch(_) = self.event_kind() {
            let mut parties = self.as_path().last().split('_');
            Some((parties.next().unwrap(), parties.next().unwrap()))
        } else {
            None
        }
    }

    pub fn event_kind(&self) -> EventKind {
        let event_kind = self
            .0
            .query()
            .expect("event ids always have a query string");
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
        let mut replaced = self.0.clone();
        replaced.set_query(Some(&kind.to_string()));
        EventId(replaced)
    }

    pub fn announcement_messages(&self, nonces: &Nonces) -> AnnouncementMessages {
        let secp256k1 = format!("{}!00{}", &self, &nonces.secp256k1);
        let ed25519 = format!("{}!00{}", &self, &nonces.ed25519);
        AnnouncementMessages { secp256k1, ed25519 }
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
        let url =
            url::Url::parse(&format!("event:{}", string)).map_err(|_| EventIdError::BadFormat)?;
        let event_kind = url.query().ok_or(EventIdError::BadFormat)?;
        let path = url
            .path_segments()
            .ok_or(EventIdError::BadFormat)?
            .collect::<Vec<_>>();

        let valid_kind = match event_kind {
            "vs" | "left-win" | "right-win" => {
                let last = path.last().ok_or(EventIdError::BadFormat)?;
                match VS_RE.captures(last) {
                    Some(capture) => {
                        capture.get(0).unwrap().as_str() != capture.get(1).unwrap().as_str()
                    }
                    _ => return Err(EventIdError::BadFormat),
                }
            }
            "occur" => true,
            _ => false,
        };

        if !valid_kind {
            return Err(EventIdError::UnknownEventKind(event_kind.into()));
        }

        Ok(EventId(url))
    }
}

impl From<EventId> for String {
    fn from(eid: EventId) -> Self {
        eid.as_str().to_owned()
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
        self.as_str() == rhs
    }
}

// rust made me do it
impl PartialEq<&str> for EventId {
    fn eq(&self, rhs: &&str) -> bool {
        self.as_str() == *rhs
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathRef<'a>(&'a str);

impl<'a> PathRef<'a> {
    pub fn parent(self) -> Option<PathRef<'a>> {
        if self == Self::root() {
            return None;
        }
        self.0.rfind('/').map(|at| {
            if at == 0 {
                PathRef::root()
            } else {
                PathRef(&self.0[..at])
            }
        })
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
        PathRef("/")
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NonceAndSig<N, S> {
    pub nonce: N,
    pub signature: S,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Announcement {
    pub ed25519: NonceAndSig<<Ed25519 as Curve>::PublicNonce, <Ed25519 as Curve>::SchnorrSignature>,
    pub secp256k1:
        NonceAndSig<<Secp256k1 as Curve>::PublicNonce, <Secp256k1 as Curve>::SchnorrSignature>,
}

impl Announcement {
    pub fn nonces(&self) -> Nonces {
        Nonces {
            ed25519: self.ed25519.nonce.clone(),
            secp256k1: self.secp256k1.nonce.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AnnouncedEvent {
    pub event: Event,
    pub announcement: Announcement,
    pub attestation: Option<crate::core::Attestation>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Nonces {
    pub ed25519: <Ed25519 as Curve>::PublicNonce,
    pub secp256k1: <Secp256k1 as Curve>::PublicNonce,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnnouncementMessages {
    pub ed25519: String,
    pub secp256k1: String,
}

impl AnnouncedEvent {
    pub fn signatures(&self) -> Option<Signatures> {
        self.attestation.clone().map(|attestation| {
            let scalars = attestation.scalars;
            Signatures {
                secp256k1: Secp256k1::signature_from_scalar_and_nonce(
                    scalars.secp256k1,
                    self.announcement.secp256k1.nonce.clone(),
                ),
                ed25519: Ed25519::signature_from_scalar_and_nonce(
                    scalars.ed25519,
                    self.announcement.ed25519.nonce.clone(),
                ),
            }
        })
    }
}

pub struct Signatures {
    pub ed25519: <ed25519::Ed25519 as Curve>::SchnorrSignature,
    pub secp256k1: <secp256k1::Secp256k1 as Curve>::SchnorrSignature,
}

#[must_use]
pub fn verify_announcement(
    pubkeys: &crate::oracle::OraclePubkeys,
    event_id: &EventId,
    announcement: &Announcement,
) -> bool {
    let messages = event_id.announcement_messages(&announcement.nonces());
    Ed25519::verify_signature(
        &pubkeys.ed25519,
        &messages.ed25519.as_bytes(),
        &announcement.ed25519.signature,
    ) && Secp256k1::verify_signature(
        &pubkeys.secp256k1,
        &messages.secp256k1.as_bytes(),
        &announcement.secp256k1.signature,
    )
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
            Ok(EventId::from_str(&string)?)
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

    impl serde::Serialize for EventId {
        fn serialize<Ser: serde::Serializer>(
            &self,
            serializer: Ser,
        ) -> Result<Ser::Ok, Ser::Error> {
            serializer.collect_str(&self)
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
        assert!(EventId::from_str("/foo/bar?occur").is_ok());
        assert!(EventId::from_str("foo/bar?occur/").is_err());
        assert!(EventId::from_str("/foo?occur").is_ok());
        assert!(EventId::from_str("/foo/bar?occur").is_ok());
        assert!(EventId::from_str("/foo/bar/baz?occur").is_ok());
        assert!(EventId::from_str("/foo/23/52?occur").is_ok());
        assert!(EventId::from_str("/foo/bar/FOO_BAR?vs").is_ok());
        assert!(EventId::from_str("/foo/bar/FOO-BAR?vs").is_err());
    }

    #[test]
    fn event_id_parent() {
        let event_id = EventId::from_str("/one/two/three?occur").unwrap();
        assert_eq!(event_id.as_path().as_str(), "/one/two/three");
        assert_eq!(event_id.as_path().parent().unwrap().as_str(), "/one/two");
        assert_eq!(
            event_id
                .as_path()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .as_str(),
            "/one",
        );
        assert_eq!(
            event_id
                .as_path()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .as_str(),
            "/"
        );
    }
}
