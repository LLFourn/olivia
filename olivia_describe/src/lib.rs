#[macro_use]
extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;
use olivia_core::{EventId, EventKind, Path, PredicateKind, VsMatchKind, Outcome};
use core::str::FromStr;

use wasm_bindgen::prelude::*;

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

struct Houtcome(Outcome);
struct Heventid(EventId);

impl core::fmt::Display for Houtcome {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<b class='oracle-outcome'>{}</b>", self.0.outcome_string())
    }
}

impl core::fmt::Display for Heventid {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<a class='oracle-event-id' href='{}'>{}</a>",self.0, self.0)
    }
}



#[wasm_bindgen]
pub fn path_short(path: &str) -> Option<String> {
    let path = Path::from_str(path).ok()?;
    let segments = path.as_path_ref().segments().collect::<Vec<_>>();
    let desc =  match &segments[..] {
        [competition, "match", tail @ ..] => {
            match tail {
                [] => format!("Matches in the {}", lookup_competition(competition)),
                [date] => format!("Matches in the {} set to be played on {}", lookup_competition(competition), date),
                [date, teams] => {
                    let mut teams = teams.split('_');
                    let left = lookup_team(competition,teams.next()?);
                    let right = lookup_team(competition, teams.next()?);
                    let competition = lookup_competition(competition);
                    format!("{} vs {} in the {} on {}", left, right, competition, date)
                }
                _ => return None
            }
        },
        ["random"] => "Events with a random outcome chosen by the oracle".into(),
        ["time" ] => "Events that mark the passage of time".into(),
        ["time", time, ..] => format!("Events that indicate when {} has passed", time),
        ["random", time, .. ] => format!("Events whose outcome will be randomly chosen at {}", time),
        _ => return None
    };

    return Some(desc)
}


#[wasm_bindgen]
pub fn path_html(path: &str) -> Option<String> {
    let path = Path::from_str(path).ok()?;
    let segments = path.as_path_ref().segments().collect::<Vec<_>>();
    let desc = match &segments[..] {
        ["random"] => include_str!("html/random.html").into(),
        _ => return path_short(path.as_str()).map(|s| s + "."),
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
        ([competition, "match", date, _], EventKind::VsMatch(vs_kind)) => {
            let (left,right) = event_id.parties()?;
            let left = lookup_team(competition, left);
            let right = lookup_team(competition, right);
            let competition = lookup_competition(competition);
            match vs_kind  {
                VsMatchKind::WinOrDraw => format!("{} vs {} in the {} on {} (possibly a draw)", left, right, competition, date),
                VsMatchKind::Win => format!("Whether {} wins in their match against {} in the {} on {}", left, right, competition, date),
            }
        },
        (["time", datetime], EventKind::SingleOccurrence) => format!("Indicates when {} UTC has passed", datetime),
        (["random", datetime, ..], _) => format!("Outcome randomly selected from {} possibilities at {}", event_id.n_outcomes(), datetime),
        (_, EventKind::SingleOccurrence) => format!("Indicates when {} has transpired", event_id.path()),
        ([..], EventKind::VsMatch(vs_kind)) => {
            let (left,right) = event_id.parties()?;
            match vs_kind {
                VsMatchKind::WinOrDraw => format!("The result (including possibly a draw) of the competition between {} and {} specified by {}", left, right, event_id.path().parent()?),
                VsMatchKind::Win => format!("The winner of {} vs {} in whatever is described by {}", left, right, event_id.path().parent()?),
            }
        },
        ([..], EventKind::Predicate { inner, kind: PredicateKind::Eq(value) }) => format!("Whether the outcome of {} is {}", event_id.replace_kind(*inner), value),
    };
    Some(desc)
}

#[wasm_bindgen]
pub fn event_html(id: &str) -> Option<String> {
    let id = EventId::from_str(id).ok()?;
    let segments = id.path().segments().collect::<Vec<_>>();
    let kind = id.event_kind();
    match (&segments[..], kind) {
        (["random", datetime, ..], _) => Some(format!("This event has no real world meaning. The outcome will randomly be selected from the <b>{}</b> possibilities at <b>{}</b>.", id.n_outcomes(), datetime)),
        ([competition, "match", date, _], EventKind::VsMatch(vs_kind)) => {
            let (left,right) = id.parties()?;
            let left_long = lookup_team(competition, left);
            let right_long = lookup_team(competition, right);
            let competition = lookup_competition(competition);

            Some(match vs_kind {
                VsMatchKind::WinOrDraw => format!("{} vs {} in {} on {}.", left_long, right_long, competition, date) +
                    &format!(" If {} wins the oracle will attest {}.", left_long, Houtcome(Outcome { value: 0, id: id.clone() })) +
                    &format!(" If {} wins the oracle will attest {}.", right_long, Houtcome(Outcome { value: 1, id: id.clone() })) +
                    &format!(" Otherwise the oracle will attest {}.", Houtcome(Outcome { value: 2, id: id.clone() })),
                VsMatchKind::Win => format!("The winner from {} vs {} in the {} on {}.", left_long, right_long, competition, date) +
                    &format!("If {} wins then the oracle will attest {}.", left_long, Houtcome(Outcome { value: 0, id: id.clone() })) +
                    &format!("If {} wins then the oracle will attest <b>{}</b>.", right_long, Houtcome(Outcome { value: 1, id: id.clone() }))
            })

        },
        (_, EventKind::Predicate { inner, kind: PredicateKind::Eq(value) }) => {
            let ref_event_id = id.replace_kind(*inner);
            let inner_html = event_html(ref_event_id.as_str());
            Some(format!("This event asserts that the outcome of {} will be {}.", Heventid(ref_event_id), Houtcome(Outcome::try_from_id_and_outcome(id, &value).ok()?))
                 + &match inner_html {
                     Some(inner_html) => format!(" That event is described as: <blockquote>{}</blockquote>", inner_html),
                     None => "".to_string()
                 })
        },
        _ => event_short(id.as_str()).map(|s| s + ".")
    }
}


#[wasm_bindgen]
pub fn long_path_name(path: &str) -> Option<String> {
    let path = Path::from_str(path).ok()?;
    let segments = path.as_path_ref().segments().collect::<Vec<_>>();
    Some(match &segments[..] {
        [ "EPL" ] => "English Premier League".into(),
        [competition, "match", _date, teams] => {
            let (left, right) =  { let mut t = teams.split('_'); (t.next()?, t.next()?)};
            format!("{} vs {}", lookup_team(competition, left), lookup_team(competition, right))
        },
        _ => segments.get(segments.len() - 1)?.to_string()
    })
}

fn lookup_competition(name: &str) -> &str {
    match name {
        "EPL" => "the English Premier League",
        _ => name
    }
}



fn lookup_team<'a>(competition: &str, name: &'a str) -> &'a str {
    match (competition,name) {
        ("EPL","BRE") => "Brentford",
        ("EPL","ARS") => "Arsenal",
        ("EPL","MUN") => "Manchested United",
        ("EPL","LEE") => "Leeds United",
        ("EPL","BUR") => "Burnley",
        ("EPL","BHA") => "Brighton and Hove Albion",
        ("EPL","CHE") => "Chelsea",
        ("EPL","CRY") => "Crystal Palace",
        ("EPL","EVE") => "Everton",
        ("EPL","SOU") => "Southampton",
        ("EPL","LEI") => "Leicester City",
        ("EPL","WOL") => "Wolverhampton Wanderers",
        ("EPL","WAT") => "Watford",
        ("EPL","AVL") => "Aston Villa",
        ("EPL","NOR") => "Norwich City",
        ("EPL","LIV") => "Liverpool",
        ("EPL","NEW") => "Newcastle United",
        ("EPL","WHU") => "West Ham United",
        ("EPL","TOT") => "Tottenham Hotspur",
        ("EPL","MCI") => "Manchester City",
        _ => name
    }
}
