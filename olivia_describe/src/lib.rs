#[macro_use]
extern crate alloc;
use alloc::{string::String, vec::Vec};
use core::str::FromStr;
use olivia_core::{
    BoundKind, EventId, EventKind, NodeKind, Outcome, Path, PathRef, Predicate, VsMatchKind,
};

#[cfg(feature = "wasm-bindgen")]
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

#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen)]
pub fn path_short_str(path: &str) -> Option<String> {
    let path = Path::from_str(path).ok()?;
    path_short(path.as_path_ref())
}

#[allow(unused)]
struct DateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl DateTime {
    pub fn parse(dt: &str) -> Option<Self> {
        let (ymd, hms) = dt.split_once('T')?;
        if let [y, m, d] = ymd.split('-').collect::<Vec<_>>().as_slice() {
            let year = u16::from_str(y).ok()?;
            let month = u8::from_str(m).ok()?;
            let day = u8::from_str(d).ok()?;
            if let [h, m, s] = hms.split(':').collect::<Vec<_>>().as_slice() {
                let hour = u8::from_str(h).ok()?;
                let minute = u8::from_str(m).ok()?;
                let second = u8::from_str(s).ok()?;
                return Some(Self {
                    year,
                    month,
                    day,
                    hour,
                    minute,
                    second,
                });
            }
        }
        None
    }
}

pub fn path_short(path: PathRef<'_>) -> Option<String> {
    let segments = path.segments().collect::<Vec<_>>();
    let desc = match &segments[..] {
        ["s"] => format!("sport and Esport competitions"),
        ["s", competition] => lookup_competition(competition).to_string(),
        ["s", competition, "match", tail @ ..] => match tail {
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
        ["random"] => "events with a random outcome chosen by the oracle".into(),
        ["time"] => "events that mark the passage of time".into(),
        ["time", time, ..] => format!("events that indicate when {} has passed", time),
        ["random", time, ..] => format!("events whose outcome will be randomly chosen at {}", time),
        ["x"] => "exchange rates and prices".to_string(),
        ["x", exchange] => format!("exchange rates and prices on {}", exchange),
        ["x", exchange, instrument, time] if DateTime::parse(time).is_some() => {
            format!("{} on {} at {}", instrument, exchange, time)
        }
        ["x", exchange, instrument] => format!("{} on {}", instrument, exchange),
        _ => return None,
    };

    return Some(desc);
}

#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen)]
pub fn path_html_str(path: &str) -> Option<String> {
    let path = Path::from_str(path).ok()?;
    let segments = path.as_path_ref().segments().collect::<Vec<_>>();
    let desc = match &segments[..] {
        ["random"] => include_str!("html/random.html").into(),
        ["x", exchange] => format!(
            "Exchange rates and prices on <b>{}</b>",
            exchange_link(exchange)
        ),
        ["x", exchange, instrument, time] if DateTime::parse(time).is_some() => format!(
            "<b>{}</b> on <b>{}</b> at <b>{}</b>",
            instrument_link(exchange, instrument),
            exchange_link(exchange),
            time
        ),
        ["x", exchange, instrument] => format!(
            "<b>{}</b> on <b>{}</b>",
            instrument_link(exchange, instrument),
            exchange_link(exchange)
        ),

        _ => return path_short(path.as_path_ref()),
    };

    Some(desc)
}

#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen)]
#[rustfmt::skip]
pub fn event_short_str(event_id: &str) -> Option<String> {
    let event_id = EventId::from_str(event_id).ok()?;
    Some(event_short(&event_id))
}

pub fn event_short(event_id: &EventId) -> String {
    // the concept here is that each  description should make sense if "the" is prefixed to it.
    let segments = event_id.path().segments().collect::<Vec<_>>();
    let kind = event_id.event_kind();
    let desc = match (&segments[..], kind) {
        (["s", competition, "match", date, _], EventKind::VsMatch(vs_kind)) => {
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
        (["time", datetime], EventKind::SingleOccurrence) => {
            format!("time {} has passed", datetime)
        }
        (["random", datetime, ..], _) => format!(
            "oracle's randomly selected outcome from {} possibilities at {}",
            event_id.n_outcomes(),
            datetime
        ),
        (_, EventKind::SingleOccurrence) => format!("{} has transpired", event_id.path()),
        (
            ["x", exchange, instrument, time],
            EventKind::Price {
                n_digits: _n_digits,
            },
        ) => {
            format!("price of {} on {} at {}", instrument, exchange, time,)
        }
        (
            [..],
            EventKind::Price {
                n_digits: _n_digits,
            },
        ) => format!("price of {}", event_id.path()),
        ([..], EventKind::Predicate { inner, predicate }) => {
            let inner_id = event_id.replace_kind(*inner);
            match predicate {
                Predicate::Eq(value) => {
                    let outcome = Outcome::try_from_id_and_outcome(inner_id, &value)
                        .expect("this will be valid since predicate is valid");
                    format!("assertion that {}", crate::outcome(&outcome).positive,)
                }
                Predicate::Bound(bound_kind, bound) => match bound_kind {
                    olivia_core::BoundKind::Gt => {
                        format!(
                            "assertion that the {} will be greater than {}",
                            event_short(&inner_id),
                            bound
                        )
                    }
                },
            }
        }
    };
    desc
}

#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen)]
pub fn event_html_str(id: &str) -> Option<String> {
    let id = EventId::from_str(id).ok()?;
    event_html(&id)
}

pub fn event_html(id: &EventId) -> Option<String> {
    let segments = id.path().segments().collect::<Vec<_>>();
    let kind = id.event_kind();
    match (&segments[..], kind) {
        (["random", datetime, ..], _) => Some(format!("This event has no real world meaning. The outcome will randomly be selected from the <b>{}</b> possibilities at <b>{}</b>", id.n_outcomes(), datetime)),
        (["s", competition, "match", date, _], EventKind::VsMatch(vs_kind)) => {
            let (left,right) = id.parties()?;
            let left_long = lookup_team(competition, left);
            let right_long = lookup_team(competition, right);
            let competition = lookup_competition(competition);

            Some(match vs_kind {
                VsMatchKind::WinOrDraw => format!("{} match {} vs {} on {}.",competition, left_long, right_long,  date) +
                    &format!(" If {} wins the oracle will attest {}.", left_long, Houtcome(Outcome { value: 0, id: id.clone() })) +
                    &format!(" If {} wins the oracle will attest {}.", right_long, Houtcome(Outcome { value: 1, id: id.clone() })) +
                    &format!(" Otherwise the oracle will attest {}", Houtcome(Outcome { value: 2, id: id.clone() })),
                VsMatchKind::Win => format!("The winner of {} match {} vs {} on {}.", competition, left_long, right_long, date) +
                    &format!("If {} wins then the oracle will attest {}.", left_long, Houtcome(Outcome { value: 0, id: id.clone() })) +
                    &format!("If {} wins then the oracle will attest {}", right_long, Houtcome(Outcome { value: 1, id: id.clone() }))
            })

        },
        (["x", exchange, instrument, time], EventKind::Price { .. }) => {
            Some(
                format!("price of <b>{}</b> on <b>{}</b> at <b>{}</b>", instrument_link(exchange, instrument), exchange_link(exchange), time)
             )
        }
        (_, EventKind::Predicate { inner, predicate }) => {
            let inner_id = id.replace_kind(*inner);

            Some(match predicate {
                Predicate::Eq(value) => {
                    let outcome = Outcome::try_from_id_and_outcome(inner_id.clone(), &value)
                        .expect("this will be valid since predicate is valid");
                    format!("Whether {}.", crate::outcome(&outcome).positive) +
                        &format!("The oracle will attest to {} if the outcome of {} is {}. Otherwise {}",
                                 Houtcome(Outcome { id: id.clone(), value: true as u64 }),
                                 Heventid(inner_id),
                                 Houtcome(outcome),
                                 Houtcome(Outcome { id: id.clone(), value: false as u64 }))
                }
                Predicate::Bound(BoundKind::Gt, bound) => {
                    format!("Whether the {} is greater than <b>{}</b>", event_html(&inner_id).unwrap_or(event_short(&inner_id)), bound)
                }
            })
        },
        _ => Some(event_short(&id) +  ".")
    }
}

pub struct OutcomeDesc {
    pub positive: String,
    pub negative: String,
}
impl OutcomeDesc {
    pub fn maybe_negate(self, negate: bool) -> Self {
        if negate {
            Self {
                positive: self.negative,
                negative: self.positive,
            }
        } else {
            self
        }
    }
}

pub fn outcome(outcome: &Outcome) -> OutcomeDesc {
    let id = &outcome.id;
    let segments = id.path().segments().collect::<Vec<_>>();
    let kind = id.event_kind();
    let outcome_str = outcome.outcome_string();

    match (&segments[..], kind) {
        (["s", competition, "match", date, _], EventKind::VsMatch(vs_kind)) => {
            let (left, right) = id.parties().unwrap();
            let left_long = lookup_team(competition, left);
            let right_long = lookup_team(competition, right);
            let competition = lookup_competition(competition);

            if outcome_str == "draw" {
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
                VsMatchKind::Win => outcome_str,
                VsMatchKind::WinOrDraw => outcome_str.strip_suffix("_win").unwrap().into(),
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
        (_, EventKind::Price { .. }) => OutcomeDesc {
            positive: format!("the price of {} is {}", event_short(id), outcome_str),
            negative: format!("the price of {} is not {}", event_short(id), outcome_str),
        },
        (_, EventKind::Predicate { inner, predicate }) => {
            let inner_event_id = id.replace_kind(*inner);
            match predicate {
                Predicate::Eq(value) => {
                    let inner_outcome = Outcome::try_from_id_and_outcome(inner_event_id, &value)
                        .expect("predicate is valid");
                    crate::outcome(&inner_outcome).maybe_negate(outcome_str == "false")
                }
                Predicate::Bound(BoundKind::Gt, upper_bound) => OutcomeDesc {
                    positive: format!(
                        "the {} is above {}",
                        event_short(&inner_event_id),
                        upper_bound
                    ),
                    negative: format!(
                        "the {} is not above {}",
                        event_short(&inner_event_id),
                        upper_bound
                    ),
                }
                .maybe_negate(outcome_str == "false"),
            }
        }
        _ => OutcomeDesc {
            positive: format!(
                "the {} is \"{}\"",
                event_short(id),
                outcome.outcome_string()
            ),
            negative: format!(
                "the {} is not \"{}\"",
                event_short(id),
                outcome.outcome_string()
            ),
        },
    }
}

#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen)]
pub fn outcome_str(id: &str, outcome: &str) -> Option<String> {
    let id = EventId::from_str(id).ok()?;
    let outcome = Outcome::try_from_id_and_outcome(id.clone(), outcome).ok()?;
    Some(crate::outcome(&outcome).positive)
}

#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen)]
pub fn long_path_name_str(path: &str) -> Option<String> {
    let path = Path::from_str(path).ok()?;
    let segments = path.as_path_ref().segments().collect::<Vec<_>>();
    Some(match &segments[..] {
        ["s"] => "Sport".into(),
        ["s", competition] => lookup_competition(competition).to_string(),
        ["s", competition, "match", _date, teams] => {
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
        ["x"] => "Exchange rates".into(),
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

fn exchange_url(exchange: &str) -> Option<&'static str> {
    Some(match exchange {
        "BitMEX" => "https://bitmex.com",
        "Binance" => "https://binance.com",
        "FTX" => "https://ftx.com",
        _ => return None,
    })
}

fn exchange_link(exchange: &str) -> String {
    match exchange_url(exchange) {
        Some(url) => format!(r#"<a href="{}">{}</a>"#, url, exchange),
        _ => exchange.to_string(),
    }
}

fn instrument_url(exchange: &str, instrument: &str) -> Option<&'static str> {
    Some(match (exchange, instrument) {
        ("BitMEX", "BXBT") => "https://www.bitmex.com/app/index/.BXBT",
        _ => return None,
    })
}

fn instrument_link(exchange: &str, instrument: &str) -> String {
    match instrument_url(exchange, instrument) {
        Some(url) => format!(r#"<a href="{}">{}</a>"#, url, instrument),
        _ => instrument.to_string(),
    }
}

pub fn infer_node_kind(path: PathRef<'_>) -> NodeKind {
    let segments = path.segments().collect::<Vec<_>>();
    match &segments[..] {
        ["s", _competition, "match"] => NodeKind::DateMap,
        _ => NodeKind::List,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_describe_outcome_for_competition_match() {
        let event_id = "/s/EPL/match/2021-08-13/BRE_ARS.vs";
        let predicated = "/s/EPL/match/2021-08-13/BRE_ARS.vs=ARS_win";
        let predicated_draw = "/s/EPL/match/2021-08-13/BRE_ARS.vs=draw";
        assert_eq!(
            outcome_str(&event_id, "BRE_win").unwrap(),
            "Brentford beats Arsenal in their English Premier League match on 2021-08-13"
        );
        assert_eq!(
            outcome_str(&event_id, "ARS_win").unwrap(),
            "Arsenal beats Brentford in their English Premier League match on 2021-08-13"
        );
        assert_eq!(
            outcome_str(&event_id, "draw").unwrap(),
            "Brentford and Arsenal draw in their English Premier League match on 2021-08-13"
        );
        assert_eq!(
            outcome_str(&predicated, "true").unwrap(),
            "Arsenal beats Brentford in their English Premier League match on 2021-08-13"
        );
        assert_eq!(
            outcome_str(&predicated, "false").unwrap(),
            "Arsenal does not beat Brentford in their English Premier League match on 2021-08-13"
        );
        assert_eq!(
            outcome_str(&predicated_draw, "true").unwrap(),
            "Brentford and Arsenal draw in their English Premier League match on 2021-08-13"
        );
        assert_eq!(
            outcome_str(&predicated_draw, "false").unwrap(),
            "Brentford and Arsenal do not draw in their English Premier League match on 2021-08-13"
        );
    }

    #[test]
    fn test_x_path() {
        assert_eq!(
            path_short_str("/x/BitMEX"),
            Some("exchange rates and prices on BitMEX".into())
        );
        assert_eq!(
            path_short_str("/x/BitMEX/BXBT"),
            Some("BXBT on BitMEX".into())
        );
        assert_eq!(
            path_short_str("/x/BitMEX/BXBT/2021-10-05T5:00:00"),
            Some("BXBT on BitMEX at 2021-10-05T5:00:00".into())
        );
    }

    #[test]
    fn test_price_event_short() {
        assert_eq!(
            event_short_str("/x/BitMEX/BXBT/2021-10-05T5:00:00.price"),
            Some("price of BXBT on BitMEX at 2021-10-05T5:00:00".into())
        );

        assert_eq!(
            event_short_str("/x/BitMEX/BXBT/2021-10-05T5:00:00.price?n=20"),
            Some("price of BXBT on BitMEX at 2021-10-05T5:00:00".into())
        );
    }

    #[test]
    fn test_bounded_price_event() {
        assert_eq!(
            event_short_str("/x/BitMEX/BXBT/2021-10-05T5:00:00.price_10000"),
            Some("assertion that the price of BXBT on BitMEX at 2021-10-05T5:00:00 is greater than 10000".into())
        );
    }
}
