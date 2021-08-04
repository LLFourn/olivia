#[macro_use]
extern crate alloc;
use alloc::{string::String, vec::Vec};
use core::str::FromStr;
use olivia_core::{EventId, EventKind, Outcome, Path, PredicateKind, VsMatchKind};

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
        write!(
            f,
            "<b class='oracle-outcome'>{}</b>",
            self.0.outcome_string()
        )
    }
}

impl core::fmt::Display for Heventid {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "<a class='oracle-event-id' href='{}'>{}</a>",
            self.0, self.0
        )
    }
}

#[wasm_bindgen]
pub fn path_short(path: &str) -> Option<String> {
    let path = Path::from_str(path).ok()?;
    let segments = path.as_path_ref().segments().collect::<Vec<_>>();
    let desc = match &segments[..] {
        [competition, "match", tail @ ..] => match tail {
            [] => format!("{} matches", lookup_competition(competition)),
            [date] => format!(
                "{} matches set to be played on {}",
                lookup_competition(competition),
                date
            ),
            [date, teams] => {
                let mut teams = teams.split('_');
                let left = lookup_team(competition, teams.next()?);
                let right = lookup_team(competition, teams.next()?);
                let competition = lookup_competition(competition);
                format!("{} match {} vs {} on {}", competition, left, right, date)
            }
            _ => return None,
        },
        ["random"] => "Events with a random outcome chosen by the oracle".into(),
        ["time"] => "Events that mark the passage of time".into(),
        ["time", time, ..] => format!("Events that indicate when {} has passed", time),
        ["random", time, ..] => format!("Events whose outcome will be randomly chosen at {}", time),
        _ => return None,
    };

    return Some(desc);
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
    Some(event_short_id(&event_id))
}

pub fn event_short_id(event_id: &EventId) -> String {
    // the concept here is that each  description should make sense if "the" is prefixed to it.
    let segments = event_id.path().segments().collect::<Vec<_>>();
    let kind = event_id.event_kind();
    let desc = match (&segments[..], kind) {
        ([competition, "match", date, _], EventKind::VsMatch(vs_kind)) => {
            let (left, right) = event_id.parties().unwrap();
            let left_long = lookup_team(competition, left);
            let right_long = lookup_team(competition, right);
            let competition = lookup_competition(competition);
            match vs_kind {
                VsMatchKind::WinOrDraw => format!(
                    "result of {} match {} vs {} on {}",
                    competition, left_long, right_long, date
                ),
                VsMatchKind::Win => format!(
                    "winner of {} match {} vs {} on {}",
                    competition, left_long, right_long, date
                ),
            }
        }
        (["time", datetime], EventKind::SingleOccurrence) => {
            format!("time {} has passed", datetime)
        }
        (["random", datetime, ..], _) => format!(
            "randomly selected outcome from {} possibilities at {}",
            event_id.n_outcomes(),
            datetime
        ),
        (_, EventKind::SingleOccurrence) => format!("{} has transpired", event_id.path()),
        ([..], EventKind::VsMatch(vs_kind)) => {
            let (left, right) = event_id.parties().unwrap();
            match vs_kind {
                VsMatchKind::WinOrDraw => format!(
                    "result of {} vs {} in {}",
                    left,
                    right,
                    event_id.path().parent().unwrap()
                ),
                VsMatchKind::Win => format!(
                    "winner of {} vs {} in {}",
                    left,
                    right,
                    event_id.path().parent().unwrap()
                ),
            }
        }
        (
            [..],
            EventKind::Predicate {
                inner,
                kind: PredicateKind::Eq(value),
            },
        ) => {
            let inner_id = EventId::from_path_and_kind(event_id.path().to_path(), *inner);
            format!(
                "assertion that the {} will be {}",
                event_short_id(&inner_id),
                value
            )
        }
    };
    desc
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
                VsMatchKind::WinOrDraw => format!("{} match {} vs {} on {}.",competition, left_long, right_long,  date) +
                    &format!(" If {} wins the oracle will attest {}.", left_long, Houtcome(Outcome { value: 0, id: id.clone() })) +
                    &format!(" If {} wins the oracle will attest {}.", right_long, Houtcome(Outcome { value: 1, id: id.clone() })) +
                    &format!(" Otherwise the oracle will attest {}.", Houtcome(Outcome { value: 2, id: id.clone() })),
                VsMatchKind::Win => format!("The winner from {} match {} vs {} on {}.", competition, left_long, right_long, date) +
                    &format!("If {} wins then the oracle will attest {}.", left_long, Houtcome(Outcome { value: 0, id: id.clone() })) +
                    &format!("If {} wins then the oracle will attest {}.", right_long, Houtcome(Outcome { value: 1, id: id.clone() }))
            })

        },
        (_, EventKind::Predicate { inner, kind: PredicateKind::Eq(value) }) => {
            let inner_event_id = id.replace_kind(*inner);
            let inner_html = event_html(inner_event_id.as_str());
            let outcome = Outcome::try_from_id_and_outcome(inner_event_id.clone(), &value).ok()?;
            Some(format!("This event asserts that the outcome of {} will be {}.", Heventid(inner_event_id), Houtcome(outcome))
                 + &match inner_html {
                     Some(inner_html) => format!(" That event is described as: <blockquote>{}</blockquote>", inner_html),
                     None => "".to_string()
                 })
        },
        _ => event_short(id.as_str()).map(|s| s + ".")
    }
}

pub struct OutcomeDesc {
    pub positive: String,
    pub negative: String,
}
impl OutcomeDesc {
    pub fn negate(self) -> Self {
        Self {
            positive: self.negative,
            negative: self.positive,
        }
    }
}

pub fn _describe_outcome(id: EventId, outcome: &str) -> OutcomeDesc {
    let segments = id.path().segments().collect::<Vec<_>>();
    let kind = id.event_kind();

    match (&segments[..], kind) {
        ([competition, "match", date, _], EventKind::VsMatch(vs_kind)) => {
            let (left, right) = id.parties().unwrap();
            let left_long = lookup_team(competition, left);
            let right_long = lookup_team(competition, right);
            let competition = lookup_competition(competition);

            if outcome == "draw" {
                return OutcomeDesc {
                    positive: format!(
                        "{} and {} draw in their {} match on {}",
                        left_long, right_long, competition, date
                    ),
                    negative: format!(
                        "{} and {} do not draw in their {} match on {}",
                        left_long, right_long, competition, date
                    ),
                };
            }

            let winner = match vs_kind {
                VsMatchKind::Win => outcome,
                VsMatchKind::WinOrDraw => outcome.strip_suffix("_win").unwrap().into(),
            };

            let (winner, loser) = if winner == left {
                (left_long, right_long)
            } else {
                (right_long, left_long)
            };

            OutcomeDesc {
                positive: format!(
                    "{} beats {} in their {} match on {}",
                    winner, loser, competition, date
                ),
                negative: format!(
                    "{} does not beat {} in their {} match on {}",
                    winner, loser, competition, date
                ),
            }
        }
        (
            _,
            EventKind::Predicate {
                inner,
                kind: PredicateKind::Eq(value),
            },
        ) => {
            let inner_event_id = id.replace_kind(*inner);
            if outcome == "true" {
                _describe_outcome(inner_event_id, &value)
            } else {
                _describe_outcome(inner_event_id, &value).negate()
            }
        }
        _ => OutcomeDesc {
            positive: format!("the {} is {}", event_short_id(&id), outcome),
            negative: format!("the {} is not {}", event_short_id(&id), outcome),
        },
    }
}

#[wasm_bindgen]
pub fn describe_outcome(id: &str, outcome: &str) -> Option<String> {
    let id = EventId::from_str(id).ok()?;
    let _ = Outcome::try_from_id_and_outcome(id.clone(), outcome).ok()?;
    Some(_describe_outcome(id, outcome).positive)
}

#[wasm_bindgen]
pub fn long_path_name(path: &str) -> Option<String> {
    let path = Path::from_str(path).ok()?;
    let segments = path.as_path_ref().segments().collect::<Vec<_>>();
    Some(match &segments[..] {
        ["EPL"] => "English Premier League".into(),
        [competition, "match", _date, teams] => {
            let (left, right) = {
                let mut t = teams.split('_');
                (t.next()?, t.next()?)
            };
            format!(
                "{} vs {}",
                lookup_team(competition, left),
                lookup_team(competition, right)
            )
        }
        _ => segments.get(segments.len() - 1)?.to_string(),
    })
}

fn lookup_competition(name: &str) -> &str {
    match name {
        "EPL" => "English Premier League",
        _ => name,
    }
}

fn lookup_team<'a>(competition: &str, name: &'a str) -> &'a str {
    match (competition, name) {
        ("EPL", "BRE") => "Brentford",
        ("EPL", "ARS") => "Arsenal",
        ("EPL", "MUN") => "Manchested United",
        ("EPL", "LEE") => "Leeds United",
        ("EPL", "BUR") => "Burnley",
        ("EPL", "BHA") => "Brighton and Hove Albion",
        ("EPL", "CHE") => "Chelsea",
        ("EPL", "CRY") => "Crystal Palace",
        ("EPL", "EVE") => "Everton",
        ("EPL", "SOU") => "Southampton",
        ("EPL", "LEI") => "Leicester City",
        ("EPL", "WOL") => "Wolverhampton Wanderers",
        ("EPL", "WAT") => "Watford",
        ("EPL", "AVL") => "Aston Villa",
        ("EPL", "NOR") => "Norwich City",
        ("EPL", "LIV") => "Liverpool",
        ("EPL", "NEW") => "Newcastle United",
        ("EPL", "WHU") => "West Ham United",
        ("EPL", "TOT") => "Tottenham Hotspur",
        ("EPL", "MCI") => "Manchester City",
        _ => name,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_describe_outcome_for_competition_match() {
        let event_id = "/EPL/match/2021-08-13/BRE_ARS.vs";
        let predicated = "/EPL/match/2021-08-13/BRE_ARS.vs=ARS_win";
        let predicated_draw = "/EPL/match/2021-08-13/BRE_ARS.vs=draw";
        assert_eq!(
            describe_outcome(&event_id, "BRE_win").unwrap(),
            "Brentford beats Arsenal in their English Premier League match on 2021-08-13"
        );
        assert_eq!(
            describe_outcome(&event_id, "ARS_win").unwrap(),
            "Arsenal beats Brentford in their English Premier League match on 2021-08-13"
        );
        assert_eq!(
            describe_outcome(&event_id, "draw").unwrap(),
            "Brentford and Arsenal draw in their English Premier League match on 2021-08-13"
        );
        assert_eq!(
            describe_outcome(&predicated, "true").unwrap(),
            "Arsenal beats Brentford in their English Premier League match on 2021-08-13"
        );
        assert_eq!(
            describe_outcome(&predicated, "false").unwrap(),
            "Arsenal does not beat Brentford in their English Premier League match on 2021-08-13"
        );
        assert_eq!(
            describe_outcome(&predicated_draw, "true").unwrap(),
            "Brentford and Arsenal draw in their English Premier League match on 2021-08-13"
        );
        assert_eq!(
            describe_outcome(&predicated_draw, "false").unwrap(),
            "Brentford and Arsenal do not draw in their English Premier League match on 2021-08-13"
        );
    }
}
