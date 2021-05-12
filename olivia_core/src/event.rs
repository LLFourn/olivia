use crate::Descriptor;
use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use chrono::NaiveDateTime;
use core::{fmt, str::FromStr};

#[derive(Debug, Clone, PartialEq)]
pub enum EventKind {
    VsMatch(VsMatchKind),
    SingleOccurrence,
    Digits(u8),
}

#[derive(Debug, Clone, PartialEq)]
pub enum VsMatchKind {
    WinOrDraw,
    Win,
}

impl fmt::Display for EventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EventKind::VsMatch(kind) => match kind {
                VsMatchKind::Win => write!(f, "win"),
                VsMatchKind::WinOrDraw => write!(f, "vs"),
            },
            EventKind::SingleOccurrence => write!(f, "occur"),
            EventKind::Digits(n) => write!(f, "digits_{}", n),
        }
    }
}

impl EventKind {
    pub fn n_nonces(&self) -> u8 {
        match self {
            EventKind::Digits(n) => *n,
            _ => 1,
        }
    }
}

#[derive(Clone, PartialEq, Hash, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "diesel", derive(diesel::AsExpression, diesel::FromSqlRow))]
#[cfg_attr(feature = "diesel", sql_type = "diesel::sql_types::Text")]
pub struct EventId(String);

impl EventId {
    pub fn as_bytes(&self) -> &[u8] {
        self.as_str().as_bytes()
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn path(&self) -> PathRef<'_> {
        let (path, _) = PathRef(self.as_str()).strip_event().expect("event must exist");
        path
    }

    pub fn parties(&self) -> Option<(&str, &str)> {
        if let EventKind::VsMatch(_) = self.event_kind() {
            let mut parties = self.path().last().split('_');
            Some((parties.next().unwrap(), parties.next().unwrap()))
        } else {
            None
        }
    }

    pub fn unit(&self) -> Option<String> {
        None
    }

    pub fn event_kind(&self) -> EventKind {
        let (_, event_kind) = PathRef(self.as_str()).strip_event().expect("event must exist");
        let event_kind_segments: Vec<&str> = event_kind.split('_').collect::<Vec<_>>();
        match &event_kind_segments[..] {
            ["vs"] => EventKind::VsMatch(VsMatchKind::WinOrDraw),
            ["win"] => EventKind::VsMatch(VsMatchKind::Win),
            ["occur"] => EventKind::SingleOccurrence,
            ["digits", n] => {
                EventKind::Digits(u8::from_str(n).expect("we've checked this already"))
            }
            this => unreachable!(
                "valid event ids have already been checked to not be {}",
                this.join("_")
            ),
        }
    }

    pub fn n_outcomes_for_nonce(&self, _nonce_index: usize) -> u32 {
        match self.event_kind() {
            EventKind::VsMatch(kind) => match kind {
                VsMatchKind::WinOrDraw => 3,
                _ => 2,
            },
            EventKind::SingleOccurrence => 1,
            EventKind::Digits(_n) => unimplemented!(),
        }
    }

    pub fn n_outcomes(&self) -> u64 {
        match self.event_kind() {
            EventKind::Digits(_n) => unimplemented!(),
            _ => self.n_outcomes_for_nonce(0) as u64,
        }
    }

    pub fn replace_kind(&self, kind: EventKind) -> EventId {
        Self(format!("{}.{}", self.path(), kind))
    }

    pub fn descriptor(&self) -> Descriptor {
        match self.event_kind() {
            EventKind::VsMatch(kind) => {
                let (left, right) = self.parties().unwrap();
                let mut outcomes = vec![format!("{}_win", left), format!("{}_win", right)];
                if let VsMatchKind::WinOrDraw = kind {
                    outcomes.push("draw".into());
                }
                Descriptor::Enum { outcomes }
            }
            EventKind::SingleOccurrence => Descriptor::Enum {
                outcomes: vec!["true".into()],
            },
            EventKind::Digits(n) => Descriptor::DigitDecomposition {
                base: 10,
                is_signed: false,
                n_digits: n,
                unit: self.unit(),
            },
        }
    }

    pub fn is_binary(&self) -> bool {
        match self.event_kind() {
            EventKind::VsMatch(kind) => match kind {
                VsMatchKind::Win { .. } => true,
                _ => false,
            },
            _ => false,
        }
    }

    pub fn n_nonces(&self) -> u8 {
        self.event_kind().n_nonces()
    }
}

#[derive(Debug, Clone)]
pub enum EventIdError {
    NotAnEvent,
    BadFormat,
    UnknownEventKind(String),
}

impl core::fmt::Display for EventIdError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            EventIdError::NotAnEvent => write!(f, "not a valid event id"),
            EventIdError::BadFormat => write!(f, "badly formatted event id"),
            EventIdError::UnknownEventKind(event_kind) => {
                write!(f, "{} is not a recognized event kind", event_kind)
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for EventIdError {}

impl FromStr for EventId {
    type Err = EventIdError;

    fn from_str(string: &str) -> Result<EventId, Self::Err> {
        let url =
            // test: is just there to give it a scheme which we need for the url lib to parse it
            url::Url::parse(&format!("test:{}", string)).map_err(|_| EventIdError::BadFormat)?;
        if url.path() != string {
            // sanity check -- the URL path is the evet ID so if we roundtrip it, it should come out
            // the same
            return Err(EventIdError::BadFormat)
        }
        let (path, event_kind) = PathRef::from(string).strip_event().ok_or(EventIdError::BadFormat)?;
        let event_kind_segments = event_kind.split("_").collect::<Vec<_>>();

        // Ensure the path is a valid url path
        match &event_kind_segments[..] {
            ["vs"] | ["win"] => {
                let teams: Vec<_> = path.last().split('_').collect();
                if teams.len() != 2 || teams[0] == teams[1] {
                    return Err(EventIdError::BadFormat);
                }
            }
            ["digits", n] => {
                u8::from_str(n).or(Err(EventIdError::BadFormat))?;
            }
            ["occur"] => (),
            _ => return Err(EventIdError::UnknownEventKind(event_kind.into())),
        };

        Ok(EventId(string.into()))
    }
}

impl From<EventId> for String {
    fn from(eid: EventId) -> Self {
        eid.as_str().to_string()
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

    pub fn segment(self, index: usize) -> Option<&'a str> {
        self.0[1..].split('/').nth(index)
    }

    pub fn strip_event(self) -> Option<(PathRef<'a>, &'a str)> {
        self.0.rfind('/').and_then(|slash_at| {
            let last_segment = &self.0[slash_at + 1..];
            last_segment.find('.').map(|dot_at| (PathRef(&self.0[..slash_at + 1 + dot_at]), &last_segment[dot_at + 1..]))
        })
    }

    pub fn last(self) -> &'a str {
        let last_segment = self.0
            .rfind('/')
            .map(|at| &self.0[at + 1..])
            .unwrap_or(&self.0[..]);

        last_segment
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

impl fmt::Debug for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for PathRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub id: EventId,
    pub expected_outcome_time: Option<NaiveDateTime>,
}
impl From<NaiveDateTime> for Event {
    fn from(dt: NaiveDateTime) -> Self {
        Event {
            id: EventId::from(dt),
            expected_outcome_time: Some(dt),
        }
    }
}

impl From<NaiveDateTime> for EventId {
    fn from(dt: NaiveDateTime) -> Self {
        EventId::from_str(&format!("/time/{}.occur", dt.format("%FT%T"))).unwrap()
    }
}

#[cfg(feature = "diesel")]
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

            deserializer.deserialize_str(Visitor)
        }
    }

    impl serde::Serialize for EventId {
        fn serialize<Ser: serde::Serializer>(
            &self,
            serializer: Ser,
        ) -> Result<Ser::Ok, Ser::Error> {
            let tmp = serializer.collect_str(&self);
            tmp
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn event_id_from_str() {
        assert!(EventId::from_str("/foo/bar.occur").is_ok());
        assert!(EventId::from_str("foo/bar.occur/").is_err());
        assert!(EventId::from_str("/foo.occur").is_ok());
        assert!(EventId::from_str("/foo/bar.occur").is_ok());
        assert!(EventId::from_str("/foo/bar/baz.occur").is_ok());
        assert!(EventId::from_str("/foo/23/52.occur").is_ok());
        assert!(EventId::from_str("/foo/bar/FOO_BAR.vs").is_ok());
        assert!(EventId::from_str("/foo/bar/FOO-BAR.vs").is_err());
        assert!(EventId::from_str("/foo.occur").is_ok());
        assert!(EventId::from_str("/test/one/two/3.occur").is_ok());
    }

    #[test]
    fn event_id_parent() {
        let event_id = EventId::from_str("/one/two/three.occur").unwrap();
        assert_eq!(event_id.path().as_str(), "/one/two/three");
        assert_eq!(event_id.path().parent().unwrap().as_str(), "/one/two");
        assert_eq!(
            event_id
                .path()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .as_str(),
            "/one",
        );
        assert_eq!(
            event_id
                .path()
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
