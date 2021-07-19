use olivia_core::{VsMatchKind};

pub fn event_short_vs(competition: &str, date: &str, left: &str, right: &str, vs_kind: VsMatchKind) -> String {
    let left = lookup_team(competition, left);
    let right = lookup_team(competition, right);
    let competition = lookup_competition(competition);

    match vs_kind  {
        VsMatchKind::WinOrDraw => format!("{} vs {} in the {} on {} (possibly a draw)", left, right, competition, date),
        VsMatchKind::Win => format!("Whether {} wins in their match against {} in the {} on {}", left, right, competition, date),
    }
}

pub fn path_long(segments: &[&str]) -> Option<String> {
    path_short(segments)
}

pub fn path_short(segments: &[&str]) -> Option<String> {
    Some(match segments {
        [] => "Soccer competitions".into(),
        [competition] => format!("The {}", lookup_competition(competition)),
        [competition, "match"] => format!("Matches in the {}", lookup_competition(competition)),
        [competition, "match", date] => format!("Matches in the {} set to be played on {}", lookup_competition(competition), date),
        [competition, "match",date, teams] => {
            let mut teams = teams.split('_');
            let left = lookup_team(competition,teams.next()?);
            let right = lookup_team(competition, teams.next()?);
            let competition = lookup_competition(competition);
            format!("{} vs {} in the {} on {}", left, right, competition, date)
        }
        _ => return None
    })
}

fn lookup_competition(name: &str) -> &str {
    match name {
        "EPL" => "English Premier League",
        _ => name
    }
}


// fn lookup_competition_html(name: &str) -> &str {
//     match name {
//         "EPL" => include_str!("html/soccer/EPL.html"),
//         _ => name
//     }
// }

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

#[cfg(test)]
mod test {

    #[test]
    fn test_event_short() {
        assert_eq!(crate::event_short("/soccer/EPL/match/2020-08-14/BRE_ARS.vs"),Some( "Brentford vs Arsenal in the English Premier League on 2020-08-14 (possibly a draw)".into()));
        assert_eq!(crate::event_short("/soccer/EPL/match/2020-08-14/BRE_ARS.winner"),Some( "Whether Brentford wins in their match against Arsenal in the English Premier League on 2020-08-14".into()));
    }

    #[test]
    fn test_path_short() {
        assert_eq!(crate::path_short("/soccer"),Some( "Soccer competitions".into()));
        assert_eq!(crate::path_short("/soccer/EPL"),Some("The English Premier League".into()));
        assert_eq!(crate::path_short("/soccer/EPL/match"),Some("Matches in the English Premier League".into()));
        assert_eq!(crate::path_short("/soccer/EPL/match/2020-08-14"),Some("Matches in the English Premier League set to be played on 2020-08-14".into()));
        assert_eq!(crate::path_short("/soccer/EPL/match/2020-08-14/BRE_ARS"),Some( "Brentford vs Arsenal in the English Premier League on 2020-08-14".into()));
    }

}
