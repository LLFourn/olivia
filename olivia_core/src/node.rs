use crate::{EventKind, Path, PrefixPath};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case", tag = "kind")]
pub enum ChildDesc {
    List {
        list: Vec<Child>,
    },
    Range {
        #[serde(flatten)]
        range_kind: RangeKind,
        start: Option<Child>,
        end: Option<Child>,
    },
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "range-kind", rename_all = "kebab-case")]
pub enum RangeKind {
    Time { interval: u32 },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct GetPath {
    pub events: Vec<EventKind>,
    #[serde(rename = "children")]
    pub child_desc: ChildDesc,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Child {
    pub name: String,
    #[serde(flatten)]
    pub kind: NodeKind,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum NodeKind {
    List,
    Range {
        #[serde(flatten)]
        range_kind: RangeKind,
    },
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Node {
    pub path: Path,
    pub kind: NodeKind,
}

impl PrefixPath for Node {
    fn prefix_path(mut self, path: crate::PathRef<'_>) -> Self {
        self.path = self.path.prefix_path(path);
        self
    }

    fn strip_prefix_path(mut self, path: crate::PathRef<'_>) -> Self {
        self.path = self.path.strip_prefix_path(path);
        self
    }
}
