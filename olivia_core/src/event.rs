use crate::{Descriptor, Outcome, OutcomeError, Path, PathError, PathRef, PrefixPath};
use chrono::NaiveDateTime;
use core::{convert::TryFrom, fmt, str::FromStr};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventKind {
    VsMatch(VsMatchKind),
    SingleOccurrence,
    Predicate {
        inner: Box<EventKind>,
        kind: PredicateKind,
    },
    Price {
        /// the number of nonces the oracle will use if using nonce based attestation.
        /// Can't be more than 64.
        n_digits: u8,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum PredicateKind {
    Eq(String),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum VsMatchKind {
    WinOrDraw,
    Win,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum EventKindError {
    #[error("{0} is an unknown event kind")]
    Unknown(String),
    #[error("the argument to the event kind was badly formatted")]
    ArgsBadFormat,
    #[error("the expecting any arguments for this event kind")]
    UnexpectedArgs,
}

impl EventKind {
    pub fn n_nonces(&self) -> u8 {
        match self {
            &EventKind::Price { n_digits } => n_digits,
            _ => 1,
        }
    }
}

impl fmt::Display for EventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EventKind::VsMatch(kind) => match kind {
                VsMatchKind::Win => write!(f, "winner"),
                VsMatchKind::WinOrDraw => write!(f, "vs"),
            },
            EventKind::SingleOccurrence => write!(f, "occur"),
            EventKind::Predicate { inner, kind } => match kind {
                PredicateKind::Eq(value) => write!(f, "{}={}", inner, value),
            },
            EventKind::Price { n_digits } => {
                write!(f, "price")?;
                if *n_digits > 0 {
                    write!(f, "[n:{}]", n_digits)?;
                }
                Ok(())
            }
        }
    }
}

impl FromStr for EventKind {
    type Err = EventKindError;

    fn from_str(event_kind: &str) -> Result<Self, Self::Err> {
        fn check_no_args(args: Vec<(&str, &str)>) -> Result<(), EventKindError> {
            if args.is_empty() {
                Ok(())
            } else {
                Err(EventKindError::UnexpectedArgs)
            }
        }
        let (event_kind, args) = match event_kind.find('[') {
            Some(opener) => {
                if event_kind.ends_with(']') {
                    (
                        &event_kind[..opener],
                        event_kind[opener + 1..(event_kind.len() - 1)]
                            .split(',')
                            .map(|arg| arg.split_once(':').unwrap_or((arg, "")))
                            .collect(),
                    )
                } else {
                    return Err(EventKindError::ArgsBadFormat);
                }
            }
            None => (event_kind, vec![]),
        };

        Ok(match (event_kind, args) {
            ("vs", args) => {
                check_no_args(args)?;
                EventKind::VsMatch(VsMatchKind::WinOrDraw)
            }
            ("winner", args) => {
                check_no_args(args)?;
                EventKind::VsMatch(VsMatchKind::Win)
            }
            ("occur", args) => {
                check_no_args(args)?;
                EventKind::SingleOccurrence
            }
            ("price", args) => {
                dbg!(&args);
                let n_digits = match &args[..] {
                    [("n", n_digits)] => u8::from_str(n_digits)
                        .map_err(|_| EventKindError::ArgsBadFormat)
                        .and_then(|n_digits| {
                            if n_digits == 0 || n_digits > 64 {
                                Err(EventKindError::ArgsBadFormat)
                            } else {
                                Ok(n_digits)
                            }
                        })?,
                    [] => 0,
                    _ => return Err(EventKindError::UnexpectedArgs),
                };
                EventKind::Price { n_digits }
            }
            (pred, args) if pred.contains('=') => {
                check_no_args(args)?;
                let (lhs, rhs) = pred.split_once('=').expect("we checked this already");
                let inner = Self::from_str(lhs)?;
                EventKind::Predicate {
                    inner: Box::new(inner),
                    kind: PredicateKind::Eq(rhs.into()),
                }
            }
            _ => return Err(EventKindError::Unknown(event_kind.into())),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Hash, Eq, PartialOrd, Ord)]
pub struct EventId(Path);

impl EventId {
    pub fn as_bytes(&self) -> &[u8] {
        self.as_str().as_bytes()
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn path(&self) -> PathRef<'_> {
        let (path, _) = self
            .0
            .as_path_ref()
            .strip_event()
            .expect("event must exist");
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

    pub fn event_kind(&self) -> EventKind {
        let (_, event_kind) = self
            .0
            .as_path_ref()
            .strip_event()
            .expect("event must exist");
        EventKind::from_str(event_kind)
            .expect("Event kind must be valid since this is a valid event id")
    }

    pub fn n_outcomes_for_nonce(&self, _nonce_index: usize) -> u32 {
        match self.event_kind() {
            EventKind::VsMatch(kind) => match kind {
                VsMatchKind::WinOrDraw => 3,
                _ => 2,
            },
            EventKind::SingleOccurrence => 1,
            EventKind::Predicate { .. } => 2,
            EventKind::Price { .. } => 2,
        }
    }

    pub fn n_outcomes(&self) -> u64 {
        match self.event_kind() {
            EventKind::Price { .. } => u64::MAX,
            _ => self.n_outcomes_for_nonce(0) as u64,
        }
    }

    pub fn replace_kind(&self, kind: EventKind) -> EventId {
        Self(Path(format!("{}.{}", self.path(), kind)))
    }

    pub fn descriptor(&self) -> Descriptor {
        match self.event_kind() {
            EventKind::VsMatch(kind) => {
                let (left, right) = self.parties().unwrap();
                match kind {
                    VsMatchKind::Win => Descriptor::Enum {
                        outcomes: vec![left.to_string(), right.to_string()],
                    },
                    VsMatchKind::WinOrDraw => Descriptor::Enum {
                        outcomes: vec![
                            format!("{}_win", left),
                            format!("{}_win", right),
                            "draw".into(),
                        ],
                    },
                }
            }
            EventKind::SingleOccurrence => Descriptor::Enum {
                outcomes: vec!["true".into()],
            },
            EventKind::Price { n_digits } => match n_digits {
                0 => Descriptor::MissingDescriptor,
                n_digits => Descriptor::DigitDecomposition {
                    is_signed: false,
                    n_digits,
                    unit: None,
                },
            },
            EventKind::Predicate { .. } => Descriptor::Enum {
                outcomes: vec!["true".into(), "false".into()],
            },
        }
    }

    pub fn from_path_and_kind(path: Path, kind: EventKind) -> Self {
        EventId(Path(format!("{}.{}", path, kind)))
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

    pub fn occur_from_dt(dt: NaiveDateTime) -> EventId {
        Self::from_path_and_kind(Path::from_dt(dt), EventKind::SingleOccurrence)
    }

    pub fn predicate_eq(&self, value: u64) -> EventId {
        let event_kind = self.event_kind();
        let outcome_string = Outcome {
            id: self.clone(),
            value,
        }
        .outcome_string();

        assert!(
            !matches!(event_kind, EventKind::Predicate { .. }),
            "you must not predicate a already predicated event"
        );

        self.replace_kind(EventKind::Predicate {
            inner: Box::new(event_kind),
            kind: PredicateKind::Eq(outcome_string),
        })
    }

    /// Return a shortened id with only the end of the id included.
    pub fn short_id(&self) -> String {
        format!(
            "{}.{}",
            self.path().segments().last().unwrap(),
            self.event_kind()
        )
    }
}

#[derive(Debug, Clone)]
pub enum EventIdError {
    NotAnEvent,
    BadFormat,
    Kind(EventKindError),
    MissingEventKind,
    PredicateInvalidOutcome(OutcomeError),
}

impl core::fmt::Display for EventIdError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            EventIdError::NotAnEvent => write!(f, "not a valid event id"),
            EventIdError::BadFormat => write!(f, "badly formatted event id"),
            EventIdError::MissingEventKind => write!(
                f,
                "event id is valid path but doesn't end in '.<event-kind>'"
            ),
            EventIdError::Kind(event_kind) => {
                write!(f, "{}", event_kind)
            }
            EventIdError::PredicateInvalidOutcome(OutcomeError::Invalid { outcome }) => write!(
                f,
                "='{}' is an invalid predicate since '{}' is not a vaid outcome",
                outcome, outcome
            ),
        }
    }
}

impl From<PathError> for EventIdError {
    fn from(e: PathError) -> Self {
        match e {
            PathError::BadFormat => EventIdError::BadFormat,
        }
    }
}

impl From<EventKindError> for EventIdError {
    fn from(e: EventKindError) -> Self {
        EventIdError::Kind(e)
    }
}

impl std::error::Error for EventIdError {}

impl FromStr for EventId {
    type Err = EventIdError;

    fn from_str(string: &str) -> Result<EventId, Self::Err> {
        // it must at least be a valid path
        let id_as_path = Path::from_str(string)?;

        EventId::try_from(id_as_path)
    }
}

impl TryFrom<Path> for EventId {
    type Error = EventIdError;

    fn try_from(id_as_path: Path) -> Result<Self, Self::Error> {
        // It must have a `.` in the last segment to be an event
        let (path, event_kind) = id_as_path
            .as_path_ref()
            .strip_event()
            .ok_or(EventIdError::MissingEventKind)?;

        let event_kind = EventKind::from_str(event_kind)?;

        match event_kind {
            EventKind::VsMatch(_) => {
                let teams: Vec<_> = path.last().split('_').collect();
                if teams.len() != 2 || teams[0] == teams[1] {
                    return Err(EventIdError::BadFormat);
                }
            }
            EventKind::SingleOccurrence => (),
            EventKind::Predicate { inner, kind } => match kind {
                PredicateKind::Eq(value) => {
                    let id = EventId::from_path_and_kind(path.to_path(), *inner);
                    if let Err(e) = Outcome::try_from_id_and_outcome(id, &value) {
                        return Err(EventIdError::PredicateInvalidOutcome(e));
                    }
                }
            },
            _ => { /*everything is fine */ }
        };

        Ok(EventId(id_as_path))
    }
}

impl From<EventId> for String {
    fn from(eid: EventId) -> Self {
        eid.as_str().to_string()
    }
}

impl From<EventId> for Path {
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
        self.as_str() == rhs
    }
}

// rust made me do it
impl PartialEq<&str> for EventId {
    fn eq(&self, rhs: &&str) -> bool {
        self.as_str() == *rhs
    }
}

impl fmt::Display for EventId {
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

impl Event {
    pub fn occur_event_from_dt(dt: NaiveDateTime) -> Event {
        Event {
            id: EventId::occur_from_dt(dt),
            expected_outcome_time: Some(dt),
        }
    }
}

#[cfg(feature = "postgres-types")]
mod sql_impls {
    use super::*;
    use postgres_types::{private::BytesMut, *};
    use std::{boxed::Box, error::Error};

    impl<'a> FromSql<'a> for EventId {
        fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
            EventId::from_str(FromSql::from_sql(ty, raw)?)
                .map_err(|e| Box::new(e) as Box<dyn Error + Sync + Send>)
        }

        fn accepts(ty: &Type) -> bool {
            <&str as postgres_types::FromSql>::accepts(ty)
        }
    }

    impl ToSql for EventId {
        fn to_sql(
            &self,
            ty: &Type,
            out: &mut BytesMut,
        ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
            self.as_str().to_sql(ty, out)
        }

        fn accepts(ty: &Type) -> bool {
            <&str as postgres_types::ToSql>::accepts(ty)
        }

        to_sql_checked!();
    }
}

mod serde_impl {
    use super::*;
    use serde::de;

    impl<'de> de::Deserialize<'de> for EventId {
        fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<EventId, D::Error> {
            let s = String::deserialize(deserializer)?;
            EventId::from_str(&s).map_err(de::Error::custom)
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

    impl<'de> de::Deserialize<'de> for EventKind {
        fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<EventKind, D::Error> {
            let s = String::deserialize(deserializer)?;
            EventKind::from_str(&s).map_err(de::Error::custom)
        }
    }

    impl serde::Serialize for EventKind {
        fn serialize<Ser: serde::Serializer>(
            &self,
            serializer: Ser,
        ) -> Result<Ser::Ok, Ser::Error> {
            serializer.collect_str(&self)
        }
    }
}

impl PrefixPath for EventId {
    fn prefix_path(self, path: PathRef<'_>) -> Self {
        Self(Path::from(self).prefix_path(path).into())
    }

    fn strip_prefix_path(self, path: PathRef<'_>) -> Self {
        Self(Path::from(self).strip_prefix_path(path).into())
    }
}

impl PrefixPath for Event {
    fn prefix_path(mut self, path: PathRef<'_>) -> Self {
        self.id = self.id.prefix_path(path);
        self
    }

    fn strip_prefix_path(mut self, path: PathRef<'_>) -> Self {
        self.id = self.id.strip_prefix_path(path);
        self
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn event_id_from_str() {
        assert!(EventId::from_str("/foo/bar.occur").is_ok());
        assert!(EventId::from_str("foo/bar.occur").is_err());
        assert!(EventId::from_str("/foo/bar.occur/").is_err());
        assert!(EventId::from_str("/foo.occur").is_ok());
        assert!(EventId::from_str("/foo/bar.occur").is_ok());
        assert!(EventId::from_str("/foo/bar/baz.occur").is_ok());
        assert!(EventId::from_str("/foo/23/52.occur").is_ok());
        assert!(EventId::from_str("/foo/bar/FOO_BAR.vs").is_ok());
        assert!(EventId::from_str("/foo/bar/FOO_BAR.vs=FOO_win").is_ok());
        assert!(EventId::from_str("/foo/bar/FOO_BAR.vs=BAZ_win").is_err());
        assert!(EventId::from_str("/foo/bar/FOO_BAR.winner").is_ok());
        assert!(EventId::from_str("/foo/bar/FOO-BAR.vs").is_err());
        assert!(EventId::from_str("/foo.occur").is_ok());
        assert!(EventId::from_str("/test/one/two/3.occur").is_ok());
        assert!(EventId::from_str("/foo/bar.price[n:5]").is_ok());
        assert!(EventId::from_str("/foo/bar.price[n:65]").is_err());
        assert!(EventId::from_str("/foo/bar.price[n:0]").is_err());
    }

    #[test]
    fn test_n_nonces() {
        assert_eq!(EventId::from_str("/foo/bar.occur").unwrap().n_nonces(), 1);
        assert_eq!(
            EventId::from_str("/foo/bar.price[n:5]").unwrap().n_nonces(),
            5
        );
        assert_eq!(
            EventId::from_str("/foo/bar.price[n:64]")
                .unwrap()
                .n_nonces(),
            64
        );
        // price without a nonce specifer just assumes that we are not doing nonce based
        // attestations
        assert_eq!(EventId::from_str("/foo/bar.price").unwrap().n_nonces(), 0);
    }

    #[test]
    fn path_from_str() {
        assert!(Path::from_str("/foo/bar").is_ok());
        assert!(Path::from_str("/foo/bar/").is_err());
        assert!(Path::from_str("foo/bar").is_err());
        assert!(Path::from_str("/").is_ok());
        assert!(Path::from_str("/").unwrap().as_path_ref().is_root())
    }

    #[test]
    fn event_id_parent() {
        let event_id = EventId::from_str("/one/two/three.occur").unwrap();
        assert_eq!(event_id.path().as_str(), "/one/two/three");
        assert_eq!(event_id.path().parent().unwrap().as_str(), "/one/two");
        assert_eq!(
            event_id.path().parent().unwrap().parent().unwrap().as_str(),
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

    #[test]
    fn event_id_short_id() {
        assert_eq!(
            EventId::from_str("/foo/bar/FOO_BAR.vs=FOO_win")
                .unwrap()
                .short_id(),
            "FOO_BAR.vs=FOO_win"
        );
        assert_eq!(
            EventId::from_str("/foo/bar/FOO_BAR.winner")
                .unwrap()
                .short_id(),
            "FOO_BAR.winner"
        );
    }
}
