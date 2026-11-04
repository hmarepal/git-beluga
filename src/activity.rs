//! Turns an account snapshot into plain facts about its activity and picks the day's mood.
//! Nothing here reads a clock or draws, so a snapshot always gives the same whale.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Datelike, Days, NaiveDate};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Language {
    pub name: String,
    /// GitHub's `#rrggbb` color for the language.
    pub color: String,
    /// Average share across repositories, from 0 to 1. See [`average_languages`].
    pub share: f32,
}

/// One repository's languages, as bytes of code per language.
pub struct RepoLanguages {
    pub sizes: Vec<(String, String, u64)>,
}

/// Averages languages across repositories with each repository counting equally. A repo
/// that is 83% Cuda and 17% C++ adds 0.83 and 0.17, and the totals are divided by the number
/// of repos. Repositories with no code are skipped. The biggest share comes first.
pub fn average_languages(repos: &[RepoLanguages]) -> Vec<Language> {
    let mut totals: BTreeMap<&str, (&str, f64)> = BTreeMap::new();
    let mut counted = 0.0;
    for repo in repos {
        let bytes: u64 = repo.sizes.iter().map(|(_, _, size)| size).sum();
        if bytes == 0 {
            continue;
        }
        counted += 1.0;
        for (name, color, size) in &repo.sizes {
            totals.entry(name).or_insert((color, 0.0)).1 += *size as f64 / bytes as f64;
        }
    }
    let mut languages: Vec<Language> = totals
        .into_iter()
        .map(|(name, (color, total))| Language {
            name: name.to_owned(),
            color: color.to_owned(),
            share: (total / counted) as f32,
        })
        .collect();
    languages.sort_by(|a, b| {
        b.share
            .total_cmp(&a.share)
            .then_with(|| a.name.cmp(&b.name))
    });
    languages
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub login: String,
    /// The last complete UTC day: the day the whale reacts to.
    pub observed_on: NaiveDate,
    /// Repositories that went into the language average.
    pub repos_counted: u32,
    /// How many of those are private. Their names are never recorded.
    pub private_repos_counted: u32,
    /// Biggest share first.
    pub languages: Vec<Language>,
    pub days: BTreeMap<NaiveDate, u32>,
}

/// How the whale swims today, chosen from recent activity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mood {
    /// Long quiet: barely moving, eyes shut.
    Rest,
    /// Quiet for a while: slow laps.
    Drift,
    /// A few quiet days: an easy swim.
    Cruise,
    /// Back after a gap: two big spouts hello.
    Greet,
    /// An active day: the everyday swim.
    Swim,
    /// A milestone or a great week: leaping out of the water.
    Leap,
}

impl Mood {
    pub const ALL: [Mood; 6] = [
        Mood::Rest,
        Mood::Drift,
        Mood::Cruise,
        Mood::Greet,
        Mood::Swim,
        Mood::Leap,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Mood::Rest => "rest",
            Mood::Drift => "drift",
            Mood::Cruise => "cruise",
            Mood::Greet => "greet",
            Mood::Swim => "swim",
            Mood::Leap => "leap",
        }
    }

    pub fn parse(name: &str) -> Option<Mood> {
        Mood::ALL.into_iter().find(|m| m.name() == name)
    }
}

pub const REST_AFTER_QUIET_DAYS: u32 = 60;
pub const DRIFT_AFTER_QUIET_DAYS: u32 = 14;
/// A contribution after at least this many quiet days earns a greeting.
pub const GREET_AFTER_QUIET_DAYS: u32 = 3;
pub const MILESTONES: [u64; 7] = [100, 500, 1_000, 5_000, 10_000, 25_000, 50_000];

#[derive(Clone, Debug, PartialEq)]
pub struct Activity {
    pub total: u64,
    /// Contribution-free days ending on the observation date.
    pub quiet_days: u32,
    /// Quiet days immediately before the observation date.
    pub quiet_days_before: u32,
    /// Contributions in the seven days ending on the observation date.
    pub last_seven: u64,
    /// The seven-day windows before that one, newest first, at most a year of them.
    pub past_weeks: Vec<u64>,
    /// Longest run of consecutive Monday-Sunday weeks with any contribution.
    pub longest_streak_weeks: u32,
    pub recent_streak_weeks: u32,
    /// Whole weeks since the recent streak ended; 0 while it is running.
    pub weeks_since_streak: u32,
}

impl Activity {
    /// An account with no history, as a starting point for tests and demos.
    pub fn quiet() -> Activity {
        Activity {
            total: 0,
            quiet_days: 0,
            quiet_days_before: 0,
            last_seven: 0,
            past_weeks: Vec::new(),
            longest_streak_weeks: 0,
            recent_streak_weeks: 0,
            weeks_since_streak: 0,
        }
    }

    pub fn from_snapshot(snapshot: &Snapshot) -> Activity {
        let today = snapshot.observed_on;
        let days = &snapshot.days;
        let count = |date: NaiveDate| u64::from(days.get(&date).copied().unwrap_or(0));
        let first_day = days.keys().next().copied().unwrap_or(today);
        let observed_span = (today - first_day).num_days().max(0) as u64 + 1;
        let quiet_run_from = |start: NaiveDate| {
            (0..observed_span)
                .map(|back| start - Days::new(back))
                .take_while(|date| *date >= first_day && count(*date) == 0)
                .count() as u32
        };
        let window = |end_back: u64| {
            (0..7)
                .map(|d| count(today - Days::new(end_back + d)))
                .sum::<u64>()
        };
        let (longest, recent, since) = weekly_streaks(days, first_day, today);
        Activity {
            total: days.values().map(|&c| u64::from(c)).sum(),
            quiet_days: quiet_run_from(today),
            quiet_days_before: quiet_run_from(today - Days::new(1)),
            last_seven: window(0),
            past_weeks: (1..=52u64)
                .take_while(|w| w * 7 + 6 < observed_span)
                .map(|w| window(w * 7))
                .collect(),
            longest_streak_weeks: longest,
            recent_streak_weeks: recent,
            weeks_since_streak: since,
        }
    }

    /// The first matching rule wins.
    pub fn mood(&self) -> Mood {
        if self.quiet_days >= REST_AFTER_QUIET_DAYS {
            Mood::Rest
        } else if self.quiet_days >= DRIFT_AFTER_QUIET_DAYS || self.total == 0 {
            Mood::Drift
        } else if self.quiet_days > 0 {
            Mood::Cruise
        } else if self.crossed_milestone() || self.is_great_week() {
            Mood::Leap
        } else if self.quiet_days_before >= GREET_AFTER_QUIET_DAYS {
            Mood::Greet
        } else {
            Mood::Swim
        }
    }

    fn crossed_milestone(&self) -> bool {
        let before = self.total.saturating_sub(self.last_seven);
        MILESTONES.iter().any(|&m| before < m && m <= self.total)
    }

    /// At least nine in ten of the past year's active weeks fell short of this one. Empty
    /// weeks don't count, or a sparse account would leap every time it contributed.
    fn is_great_week(&self) -> bool {
        let active: Vec<u64> = self.past_weeks.iter().copied().filter(|&w| w > 0).collect();
        let shorter = active.iter().filter(|&&w| w < self.last_seven).count();
        active.len() >= 10 && shorter * 10 >= active.len() * 9
    }

    /// Bubbles in the trail: more for each week of the current streak, up to a limit. After
    /// a streak ends, the trail thins by a few bubbles a week.
    pub fn bubbles(&self) -> u32 {
        const BASE: u32 = 5;
        let earned = (self.recent_streak_weeks * 3).min(30);
        BASE + earned.saturating_sub(self.weeks_since_streak * 4)
    }
}

/// Returns (longest, most recent, whole weeks since the most recent one ended). The week in
/// progress can extend a streak, but its silence cannot end one yet.
fn weekly_streaks(
    days: &BTreeMap<NaiveDate, u32>,
    first: NaiveDate,
    today: NaiveDate,
) -> (u32, u32, u32) {
    let monday = |d: NaiveDate| d - Days::new(u64::from(d.weekday().num_days_from_monday()));
    let this_week = monday(today);
    let active: BTreeSet<NaiveDate> = days
        .iter()
        .filter(|&(&date, &count)| count > 0 && date <= today)
        .map(|(&date, _)| monday(date))
        .collect();
    let (mut longest, mut run, mut last_run, mut last_end) = (0, 0, 0, None);
    let mut week = monday(first);
    while week <= this_week {
        if active.contains(&week) {
            run += 1;
            longest = longest.max(run);
            last_run = run;
            last_end = Some(week);
        } else {
            run = 0;
        }
        week = week + Days::new(7);
    }
    let since = last_end.map_or(0, |end| {
        let silent_weeks = (this_week - end).num_days() / 7;
        (silent_weeks - 1).max(0) as u32
    });
    (longest, last_run, since)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds activity ending 2026-11-01 (a Sunday) with counts given as days back from it.
    fn activity(span_days: u64, active: &[(u64, u32)]) -> Activity {
        let today: NaiveDate = "2026-11-01".parse().unwrap();
        let days = (0..span_days)
            .map(|back| {
                let count = active
                    .iter()
                    .find(|(b, _)| *b == back)
                    .map_or(0, |(_, c)| *c);
                (today - Days::new(back), count)
            })
            .collect();
        Activity::from_snapshot(&Snapshot {
            login: "t".into(),
            observed_on: today,
            repos_counted: 0,
            private_repos_counted: 0,
            languages: vec![],
            days,
        })
    }

    fn repo(sizes: &[(&str, u64)]) -> RepoLanguages {
        RepoLanguages {
            sizes: sizes
                .iter()
                .map(|&(n, s)| (n.to_owned(), "#000000".to_owned(), s))
                .collect(),
        }
    }

    fn share(languages: &[Language], name: &str) -> f32 {
        languages
            .iter()
            .find(|l| l.name == name)
            .map_or(0.0, |l| l.share)
    }

    #[test]
    fn one_repo_keeps_its_own_percentages() {
        let languages = average_languages(&[repo(&[("Cuda", 83), ("C++", 17)])]);
        assert!((share(&languages, "Cuda") - 0.83).abs() < 1e-6);
        assert!((share(&languages, "C++") - 0.17).abs() < 1e-6);
        assert_eq!(languages[0].name, "Cuda");
    }

    #[test]
    fn repos_count_equally_however_large_they_are() {
        let languages =
            average_languages(&[repo(&[("Jupyter", 9_000_000)]), repo(&[("Python", 10)])]);
        assert!((share(&languages, "Jupyter") - 0.5).abs() < 1e-6);
        assert!((share(&languages, "Python") - 0.5).abs() < 1e-6);
    }

    #[test]
    fn empty_repos_are_skipped_and_shares_sum_to_one() {
        let languages =
            average_languages(&[repo(&[]), repo(&[("A", 1), ("B", 3)]), repo(&[("B", 5)])]);
        assert!((share(&languages, "A") - 0.125).abs() < 1e-6);
        let sum: f32 = languages.iter().map(|l| l.share).sum();
        assert!((sum - 1.0).abs() < 1e-6);
    }

    #[test]
    fn quiet_runs_pick_the_quiet_moods() {
        let base = (200, 40);
        assert_eq!(activity(400, &[base, (1, 1)]).mood(), Mood::Cruise);
        assert_eq!(activity(400, &[base, (13, 1)]).mood(), Mood::Cruise);
        assert_eq!(activity(400, &[base, (14, 1)]).mood(), Mood::Drift);
        assert_eq!(activity(400, &[base, (59, 1)]).mood(), Mood::Drift);
        assert_eq!(activity(400, &[base, (60, 1)]).mood(), Mood::Rest);
        assert_eq!(activity(30, &[]).mood(), Mood::Drift);
    }

    #[test]
    fn returning_after_a_gap_greets_and_steady_days_swim() {
        assert_eq!(
            activity(400, &[(200, 40), (0, 1), (4, 1)]).mood(),
            Mood::Greet
        );
        assert_eq!(
            activity(400, &[(200, 40), (0, 1), (1, 1)]).mood(),
            Mood::Swim
        );
    }

    #[test]
    fn milestones_and_great_weeks_leap() {
        assert_eq!(activity(400, &[(200, 95), (0, 6)]).mood(), Mood::Leap);
        let mut steady: Vec<(u64, u32)> = (1..50).map(|w| (w * 7, 2)).collect();
        steady.push((0, 9));
        assert_eq!(activity(400, &steady).mood(), Mood::Leap);
    }

    #[test]
    fn a_sparse_account_does_not_leap_for_every_contribution() {
        assert_eq!(activity(400, &[(100, 5), (0, 4)]).mood(), Mood::Greet);
    }

    #[test]
    fn streaks_grow_the_bubble_trail_and_it_thins_after_they_end() {
        let weekly: Vec<(u64, u32)> = (0..12).map(|w| (w * 7, 1)).collect();
        let running = activity(400, &weekly);
        assert_eq!(running.longest_streak_weeks, 12);
        assert_eq!(running.bubbles(), 5 + 30);

        let ended: Vec<(u64, u32)> = (2..14).map(|w| (w * 7, 1)).collect();
        let stopped = activity(400, &ended);
        assert_eq!(stopped.recent_streak_weeks, 12);
        assert_eq!(stopped.weeks_since_streak, 1);
        assert_eq!(stopped.bubbles(), 5 + 26);
    }

    #[test]
    fn a_silent_week_in_progress_does_not_end_a_streak() {
        let today: NaiveDate = "2026-11-02".parse().unwrap(); // A Monday with nothing yet.
        let days: BTreeMap<_, _> = (0..30)
            .map(|back| {
                (
                    today - Days::new(back),
                    u32::from(back > 0 && back % 7 == 1),
                )
            })
            .collect();
        let a = Activity::from_snapshot(&Snapshot {
            login: "t".into(),
            observed_on: today,
            repos_counted: 0,
            private_repos_counted: 0,
            languages: vec![],
            days,
        });
        assert_eq!(a.weeks_since_streak, 0);
        assert!(a.recent_streak_weeks >= 4);
    }
}
