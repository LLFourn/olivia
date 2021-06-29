use alloc::{string::String, vec::Vec};
use crate::EventId;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct Children {
    pub description: ChildDesc,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ChildDesc {
    List {
        list: Vec<String>
    },
    Range {
        #[serde(flatten)]
        range_kind: RangeKind,
        start: String,
        end: String,
    }
}


#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "range-kind", rename_all = "kebab-case")]
pub enum RangeKind {
    Time { interval: u32 }
}


#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct PathNode {
    pub events: Vec<EventId>,
    pub children: Children,
}
