
mod soccer;

#[macro_use]
extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;
use olivia_core::{Path, EventId, EventKind, VsMatchKind};
use core::str::FromStr;

use wasm_bindgen::prelude::*;

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;


#[wasm_bindgen]
pub fn path_short(path: &str) -> Option<String> {
    let path = Path::from_str(path).ok()?;
    let segments = path.as_path_ref().segments().collect::<Vec<_>>();
    let desc =  match &segments[..] {
        ["soccer", tail @ ..] => crate::soccer::path_short(tail)?,
        ["random"] => "Events with a random outcome chosen by the oracle".into(),
        ["time" ] => "Events that mark the passage of time".into(),
        ["time", time, ..] => format!("Events that indicate when {} has passed", time),
        ["random", time, .. ] => format!("Events whose outcome will be randomly decided at {}", time),
        _ => return None
    };

    return Some(desc)
}


#[wasm_bindgen]
pub fn path_long(path: &str) -> Option<String> {
    let path = Path::from_str(path).ok()?;
    let segments = path.as_path_ref().segments().collect::<Vec<_>>();
    let desc = match &segments[..] {
        ["soccer", tail @ ..] => crate::soccer::path_long(tail)?,
        ["random"] => include_str!("html/random.html").into(),
        ["time" ] => "Events that mark the passage of time.".into(),
        _ => return path_short(path.as_str()),
    };

    Some(desc)
}

#[wasm_bindgen]
#[rustfmt::skip]
pub fn event_short(event_id: &str) -> Option<String> {
    let event_id = EventId::from_str(event_id).ok()?;
    let segments = event_id.path().segments().collect::<Vec<_>>();
    let kind = event_id.event_kind();
    let desc = match (&segments[..], kind) {
        (["soccer", competition, "match", date, _], EventKind::VsMatch(vs_kind)) => {
            let (left,right) = event_id.parties()?;
            crate::soccer::event_short_vs(competition, date, left, right, vs_kind)
        },
        (["time", datetime], EventKind::SingleOccurrence) => format!("Indicates when {} UTC has passed", datetime),
        (["random", datetime, ..], _) => format!("The outcome of this event will be randomly selected by the oracle from the {} possibilities at {}", event_id.n_outcomes(), datetime),
        (_, EventKind::SingleOccurrence) => format!("Indicates when the event described by {} has transpired", event_id.path()),
        ([..], EventKind::VsMatch(vs_kind)) => {
            let (left,right) = event_id.parties()?;
            match vs_kind {
                VsMatchKind::WinOrDraw => format!("The result (including possibly a draw) of the competition between {} and {} specified by {}", left, right, event_id.path().parent()?),
                VsMatchKind::Win => format!("Whether {} beats in {} in the competition specified by {}", left, right, event_id.path().parent()?),
            }
        },
        ([..], EventKind::Predicate { .. }) => unimplemented!(),
    };
    Some(desc)
}
