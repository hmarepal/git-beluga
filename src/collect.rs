//! Reads an account's languages and contribution calendar through `gh`.
//!
//! When the token belongs to the account being drawn, private repositories count too. Only
//! their languages are used; their names are never stored or printed.

use std::collections::BTreeMap;
use std::process::Command;

use chrono::{DateTime, Datelike, Days, NaiveDate, Utc};
use serde_json::Value;

use crate::activity::{RepoLanguages, Snapshot, average_languages};

const REPO_FIELDS: &str = "nodes{isFork isPrivate languages(first:50,orderBy:{field:SIZE,direction:DESC}){edges{size node{name color}}}} pageInfo{hasNextPage endCursor}";

fn graphql(query: &str, vars: &[(&str, &str)]) -> Result<Value, String> {
    let mut cmd = Command::new("gh");
    cmd.args(["api", "graphql", "-f"])
        .arg(format!("query={query}"));
    for (name, value) in vars {
        cmd.arg("-f").arg(format!("{name}={value}"));
    }
    let out = cmd
        .output()
        .map_err(|e| format!("could not run the GitHub CLI (`gh`): {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_owned());
    }
    let body: Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    Ok(body["data"].clone())
}

/// Collects everything needed to draw `login`, observed through the last complete UTC day.
pub fn collect(login: &str) -> Result<Snapshot, String> {
    let account = graphql(
        "query($login:String!){user(login:$login){createdAt}}",
        &[("login", login)],
    )?;
    let created = account["user"]["createdAt"]
        .as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .ok_or_else(|| format!("no GitHub account named {login}"))?
        .date_naive();
    let observed_on = Utc::now().date_naive() - Days::new(1);
    let (repos, private_repos_counted) = collect_repositories(login)?;
    Ok(Snapshot {
        login: login.to_owned(),
        observed_on,
        repos_counted: repos.len() as u32,
        private_repos_counted,
        languages: average_languages(&repos),
        days: collect_days(login, created, observed_on)?,
    })
}

/// The account's own repositories with code, skipping forks. Reads through `viewer` when the
/// token is the account's own, because only that connection includes private repositories.
fn collect_repositories(login: &str) -> Result<(Vec<RepoLanguages>, u32), String> {
    let is_own_token = graphql("query{viewer{login}}", &[])
        .ok()
        .and_then(|data| {
            data["viewer"]["login"]
                .as_str()
                .map(|v| v.eq_ignore_ascii_case(login))
        })
        .unwrap_or(false);
    let (query, root) = if is_own_token {
        (
            format!(
                "query($cursor:String){{viewer{{repositories(first:100,after:$cursor,ownerAffiliations:[OWNER]){{{REPO_FIELDS}}}}}}}"
            ),
            "viewer",
        )
    } else {
        (
            format!(
                "query($login:String!,$cursor:String){{user(login:$login){{repositories(first:100,after:$cursor,ownerAffiliations:[OWNER],privacy:PUBLIC){{{REPO_FIELDS}}}}}}}"
            ),
            "user",
        )
    };
    let (mut repos, mut private_count) = (Vec::new(), 0);
    let mut cursor: Option<String> = None;
    loop {
        let mut vars: Vec<(&str, &str)> = Vec::new();
        if !is_own_token {
            vars.push(("login", login));
        }
        if let Some(after) = &cursor {
            vars.push(("cursor", after));
        }
        let page = graphql(&query, &vars)?;
        let connection = &page[root]["repositories"];
        for repo in connection["nodes"].as_array().into_iter().flatten() {
            if repo["isFork"].as_bool().unwrap_or(false) {
                continue;
            }
            let sizes: Vec<(String, String, u64)> = repo["languages"]["edges"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|edge| {
                    let name = edge["node"]["name"].as_str()?;
                    let color = edge["node"]["color"].as_str().unwrap_or("#8b93a7");
                    Some((name.to_owned(), color.to_owned(), edge["size"].as_u64()?))
                })
                .collect();
            if sizes.is_empty() {
                continue;
            }
            if repo["isPrivate"].as_bool().unwrap_or(false) {
                private_count += 1;
            }
            repos.push(RepoLanguages { sizes });
        }
        if !connection["pageInfo"]["hasNextPage"]
            .as_bool()
            .unwrap_or(false)
        {
            return Ok((repos, private_count));
        }
        cursor = connection["pageInfo"]["endCursor"]
            .as_str()
            .map(str::to_owned);
    }
}

/// GitHub answers at most a year of calendar per request, so this walks year-long windows.
fn collect_days(
    login: &str,
    created: NaiveDate,
    observed_on: NaiveDate,
) -> Result<BTreeMap<NaiveDate, u32>, String> {
    const QUERY: &str = "query($login:String!,$from:DateTime!,$to:DateTime!){user(login:$login){contributionsCollection(from:$from,to:$to){contributionCalendar{weeks{contributionDays{date contributionCount}}}}}}";
    let mut days = BTreeMap::new();
    let mut start = created;
    while start <= observed_on {
        let next_year = start
            .with_year(start.year() + 1)
            .unwrap_or(start + Days::new(365));
        let end = (next_year - Days::new(1)).min(observed_on);
        let (from, to) = (format!("{start}T00:00:00Z"), format!("{end}T23:59:59Z"));
        let page = graphql(QUERY, &[("login", login), ("from", &from), ("to", &to)])?;
        let weeks = &page["user"]["contributionsCollection"]["contributionCalendar"]["weeks"];
        for week in weeks.as_array().into_iter().flatten() {
            for day in week["contributionDays"].as_array().into_iter().flatten() {
                let date = day["date"]
                    .as_str()
                    .and_then(|s| s.parse::<NaiveDate>().ok());
                if let (Some(date), Some(count)) = (date, day["contributionCount"].as_u64()) {
                    if date <= observed_on {
                        days.insert(date, count as u32);
                    }
                }
            }
        }
        start = end + Days::new(1);
    }
    Ok(days)
}
