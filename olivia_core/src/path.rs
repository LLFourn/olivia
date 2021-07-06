use core::{fmt, str::FromStr};
use alloc::{
    string::{String, ToString},
};

use crate::PrefixPath;


#[derive(Debug, Clone)]
pub enum PathError {
    BadFormat
}

impl core::fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PathError::BadFormat => write!(f, "badly formatted event path")
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for PathError {}


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
            last_segment.find('.').map(|dot_at| {
                (
                    PathRef(&self.0[..slash_at + 1 + dot_at]),
                    &last_segment[dot_at + 1..],
                )
            })
        })
    }

    pub fn last(self) -> &'a str {
        let last_segment = self
            .0
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

    pub fn to_path(self) -> Path {
        Path(self.to_string())
    }
}


impl From<PathRef<'_>> for Path {
    fn from(pathref: PathRef<'_>) -> Self {
        pathref.to_path()
    }
}

impl fmt::Display for PathRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}


impl FromStr for Path {
    type Err = PathError;

    fn from_str(string: &str) -> Result<Self, PathError> {
        let url =
            // test: is just there to give it a scheme which we need for the url lib to parse it
            url::Url::parse(&format!("test:{}", string)).map_err(|_| PathError::BadFormat)?;
        if url.path() != string || !string.starts_with('/') || (string.ends_with('/')  && string != "/") {
            // sanity check -- the URL path is the evet ID so if we roundtrip it, it should come out
            // the same
            return Err(PathError::BadFormat);
        }

        Ok(Path(string.into()))
    }
}


#[derive(Clone, Debug, PartialEq, Hash, Eq, PartialOrd, Ord)]
pub struct Path(pub (crate) String);

impl Path {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn as_path_ref(&self) -> PathRef<'_> {
        PathRef(&self.0)
    }

    pub fn root() -> Self {
        PathRef::root().to_path()
    }
}


impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}


impl PrefixPath for Path {
    fn prefix_path(mut self, path: PathRef<'_>) -> Self {
        if self.as_path_ref().is_root() {
            path.to_path()
        }
        else if path.is_root() {
            self
        }
        else {
            self.0.insert_str(0, path.as_str());
            self
        }
    }

    fn strip_prefix_path(self, path: PathRef<'_>) -> Self {
        if path.is_root() {
            self
        }
        else {
            self.0.strip_prefix(path.as_str()).map(|x| Path(x.to_string())).unwrap_or(self)
        }
    }
}


mod serde_impl {
    use super::*;
    use serde::de;
impl<'de> de::Deserialize<'de> for Path {
        fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Path, D::Error> {
            let s = String::deserialize(deserializer)?;
            Path::from_str(&s).map_err(de::Error::custom)
        }
    }

    impl serde::Serialize for Path {
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
    use crate::path;

    #[test]
    fn prefix_path() {
        assert_eq!(Path::root().prefix_path(path!("/foo")), Path::from_str("/foo").unwrap());
        assert_eq!(Path::from_str("/bar").unwrap().prefix_path(path!("/foo")), Path::from_str("/foo/bar").unwrap());
        assert_eq!(Path::from_str("/bar").unwrap().prefix_path(PathRef::root()), Path::from_str("/bar").unwrap());
    }

}
