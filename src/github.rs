use anyhow::Result;
use rusqlite::{params, Connection};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::api::{ModHit, ModPage};
use crate::net::get_json;
use crate::util;

#[derive(Clone, Debug, Default)]
pub struct Repo {
    pub owner: String,
    pub repo: String,
    pub html_url: String,
    pub description: String,
    pub stars: u64,
    pub forks: u64,
    pub language: String,
    pub license: String,
    pub default_branch: String,
    pub topics: String,
    pub readme: String,
    pub open_issues: u64,
    pub homepage: String,
    pub pushed_at: String,
}

#[derive(Clone, Debug, Default)]
pub struct GhIssue {
    pub number: i64,
    pub title: String,
    pub state: String,
    pub html_url: String,
    pub labels: String,
    pub body: String,
}

#[derive(Clone, Debug, Default)]
pub struct CompatHint {
    pub kind: String,
    pub target: String,
    pub severity: String,
    pub detail: String,
    pub issue_url: String,
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub repo: Repo,
    pub issues: Vec<GhIssue>,
    pub hints: Vec<CompatHint>,
}

#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub repos: u64,
    pub issues: u64,
    pub hints: u64,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn path() -> std::path::PathBuf {
    util::data_dir().join("github.db")
}

fn open() -> Result<Connection> {
    let conn = Connection::open(path())?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA busy_timeout=8000;
         CREATE TABLE IF NOT EXISTS pages (
            source TEXT NOT NULL,
            source_id TEXT NOT NULL,
            body TEXT,
            source_url TEXT,
            issues_url TEXT,
            wiki_url TEXT,
            discord_url TEXT,
            fetched_at INTEGER NOT NULL,
            PRIMARY KEY (source, source_id)
         );
         CREATE TABLE IF NOT EXISTS repos (
            source TEXT NOT NULL,
            source_id TEXT NOT NULL,
            owner TEXT NOT NULL,
            repo TEXT NOT NULL,
            html_url TEXT,
            description TEXT,
            stars INTEGER DEFAULT 0,
            forks INTEGER DEFAULT 0,
            language TEXT,
            license TEXT,
            default_branch TEXT,
            topics TEXT,
            readme TEXT,
            open_issues INTEGER DEFAULT 0,
            homepage TEXT,
            pushed_at TEXT,
            fetched_at INTEGER NOT NULL,
            PRIMARY KEY (source, source_id)
         );
         CREATE TABLE IF NOT EXISTS issues (
            source TEXT NOT NULL,
            source_id TEXT NOT NULL,
            number INTEGER NOT NULL,
            title TEXT,
            state TEXT,
            html_url TEXT,
            labels TEXT,
            body TEXT,
            fetched_at INTEGER NOT NULL,
            PRIMARY KEY (source, source_id, number)
         );
         CREATE TABLE IF NOT EXISTS compat_hints (
            source TEXT NOT NULL,
            source_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            target TEXT NOT NULL,
            severity TEXT,
            detail TEXT,
            issue_url TEXT,
            fetched_at INTEGER NOT NULL,
            PRIMARY KEY (source, source_id, kind, target, issue_url)
         );
         CREATE INDEX IF NOT EXISTS idx_repos_owner ON repos(owner, repo);
         CREATE INDEX IF NOT EXISTS idx_issues_state ON issues(source, source_id, state);
         CREATE INDEX IF NOT EXISTS idx_hints_kind ON compat_hints(kind, target);",
    )?;
    for col in [
        "ALTER TABLE repos ADD COLUMN open_issues INTEGER DEFAULT 0",
        "ALTER TABLE repos ADD COLUMN homepage TEXT",
        "ALTER TABLE repos ADD COLUMN pushed_at TEXT",
    ] {
        let _ = conn.execute(col, []);
    }
    Ok(conn)
}

fn js(v: Option<&Value>) -> String {
    v.and_then(|x| x.as_str()).unwrap_or("").to_string()
}

fn ju(v: Option<&Value>) -> u64 {
    v.and_then(|x| x.as_u64())
        .or_else(|| v.and_then(|x| x.as_f64()).map(|n| n as u64))
        .unwrap_or(0)
}

fn clamp(mut s: String, max: usize) -> String {
    if s.len() > max {
        let mut end = max;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        s.truncate(end);
    }
    s
}

pub fn parse_github_url(url: &str) -> Option<(String, String)> {
    let u = url.trim();
    if u.is_empty() {
        return None;
    }
    let lower = u.to_ascii_lowercase();
    let idx = lower
        .find("github.com/")
        .map(|i| i + "github.com/".len())
        .or_else(|| lower.find("github.com:").map(|i| i + "github.com:".len()))?;
    let rest = u[idx..].trim_start_matches('/');
    let mut parts = rest.split(|c: char| c == '/' || c == '?' || c == '#');
    let owner = parts.next().unwrap_or("").trim();
    let repo = parts.next().unwrap_or("").trim();
    let repo = repo.strip_suffix(".git").unwrap_or(repo);
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    let skip = [
        "orgs", "settings", "topics", "about", "login", "features", "marketplace", "explore",
        "notifications", "new", "apps", "users", "search", "sponsors",
    ];
    if skip.iter().any(|s| owner.eq_ignore_ascii_case(s)) {
        return None;
    }
    Some((owner.to_string(), repo.to_string()))
}

pub fn find_github(page: &ModPage, extra: &[&str]) -> Option<(String, String)> {
    let urls = [
        page.source_url.as_str(),
        page.issues_url.as_str(),
        page.wiki_url.as_str(),
        page.discord_url.as_str(),
    ];
    for u in urls.into_iter().chain(extra.iter().copied()) {
        if let Some(pair) = parse_github_url(u) {
            return Some(pair);
        }
    }
    None
}

pub fn save_page(source: &str, source_id: &str, page: &ModPage) {
    let Ok(conn) = open() else {
        return;
    };
    let body = clamp(page.body.clone(), 200_000);
    let _ = conn.execute(
        "INSERT OR REPLACE INTO pages
         (source, source_id, body, source_url, issues_url, wiki_url, discord_url, fetched_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            source,
            source_id,
            body,
            page.source_url,
            page.issues_url,
            page.wiki_url,
            page.discord_url,
            now()
        ],
    );
}

pub fn load_page(source: &str, source_id: &str) -> Option<ModPage> {
    let conn = open().ok()?;
    conn.query_row(
        "SELECT body, source_url, issues_url, wiki_url, discord_url FROM pages
         WHERE source=?1 AND source_id=?2",
        params![source, source_id],
        |r| {
            Ok(ModPage {
                body: r.get(0)?,
                source_url: r.get(1)?,
                issues_url: r.get(2)?,
                wiki_url: r.get(3)?,
                discord_url: r.get(4)?,
            })
        },
    )
    .ok()
}

pub fn save_repo(source: &str, source_id: &str, repo: &Repo) {
    let Ok(conn) = open() else {
        return;
    };
    let readme = clamp(repo.readme.clone(), 80_000);
    let _ = conn.execute(
        "INSERT OR REPLACE INTO repos
         (source, source_id, owner, repo, html_url, description, stars, forks, language, license,
          default_branch, topics, readme, open_issues, homepage, pushed_at, fetched_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
        params![
            source,
            source_id,
            repo.owner,
            repo.repo,
            repo.html_url,
            repo.description,
            repo.stars as i64,
            repo.forks as i64,
            repo.language,
            repo.license,
            repo.default_branch,
            repo.topics,
            readme,
            repo.open_issues as i64,
            repo.homepage,
            repo.pushed_at,
            now()
        ],
    );
}

pub fn load_repo(source: &str, source_id: &str) -> Option<Repo> {
    let conn = open().ok()?;
    conn.query_row(
        "SELECT owner, repo, html_url, description, stars, forks, language, license,
                default_branch, topics, readme, open_issues, homepage, pushed_at
         FROM repos WHERE source=?1 AND source_id=?2",
        params![source, source_id],
        |r| {
            Ok(Repo {
                owner: r.get(0)?,
                repo: r.get(1)?,
                html_url: r.get(2)?,
                description: r.get(3)?,
                stars: r.get::<_, i64>(4).unwrap_or(0) as u64,
                forks: r.get::<_, i64>(5).unwrap_or(0) as u64,
                language: r.get(6)?,
                license: r.get(7)?,
                default_branch: r.get(8)?,
                topics: r.get(9)?,
                readme: r.get(10)?,
                open_issues: r.get::<_, i64>(11).unwrap_or(0) as u64,
                homepage: r.get::<_, String>(12).unwrap_or_default(),
                pushed_at: r.get::<_, String>(13).unwrap_or_default(),
            })
        },
    )
    .ok()
}

pub fn save_issues(source: &str, source_id: &str, issues: &[GhIssue]) {
    let Ok(conn) = open() else {
        return;
    };
    let t = now();
    let _ = conn.execute(
        "DELETE FROM issues WHERE source=?1 AND source_id=?2",
        params![source, source_id],
    );
    for it in issues {
        let _ = conn.execute(
            "INSERT OR REPLACE INTO issues
             (source, source_id, number, title, state, html_url, labels, body, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                source,
                source_id,
                it.number,
                it.title,
                it.state,
                it.html_url,
                it.labels,
                clamp(it.body.clone(), 4000),
                t
            ],
        );
    }
}

pub fn load_issues(source: &str, source_id: &str) -> Vec<GhIssue> {
    let Ok(conn) = open() else {
        return Vec::new();
    };
    let Ok(mut stmt) = conn.prepare(
        "SELECT number, title, state, html_url, labels, body FROM issues
         WHERE source=?1 AND source_id=?2
         ORDER BY CASE state WHEN 'open' THEN 0 ELSE 1 END, number DESC",
    ) else {
        return Vec::new();
    };
    stmt.query_map(params![source, source_id], |r| {
        Ok(GhIssue {
            number: r.get(0)?,
            title: r.get(1)?,
            state: r.get(2)?,
            html_url: r.get(3)?,
            labels: r.get(4)?,
            body: r.get(5)?,
        })
    })
    .ok()
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

pub fn save_hints(source: &str, source_id: &str, hints: &[CompatHint]) {
    let Ok(conn) = open() else {
        return;
    };
    let t = now();
    let _ = conn.execute(
        "DELETE FROM compat_hints WHERE source=?1 AND source_id=?2",
        params![source, source_id],
    );
    for h in hints {
        let _ = conn.execute(
            "INSERT OR REPLACE INTO compat_hints
             (source, source_id, kind, target, severity, detail, issue_url, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                source,
                source_id,
                h.kind,
                h.target,
                h.severity,
                h.detail,
                h.issue_url,
                t
            ],
        );
    }
}

pub fn load_hints(source: &str, source_id: &str) -> Vec<CompatHint> {
    let Ok(conn) = open() else {
        return Vec::new();
    };
    let Ok(mut stmt) = conn.prepare(
        "SELECT kind, target, severity, detail, issue_url FROM compat_hints
         WHERE source=?1 AND source_id=?2",
    ) else {
        return Vec::new();
    };
    stmt.query_map(params![source, source_id], |r| {
        Ok(CompatHint {
            kind: r.get(0)?,
            target: r.get(1)?,
            severity: r.get(2)?,
            detail: r.get(3)?,
            issue_url: r.get(4)?,
        })
    })
    .ok()
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

pub fn stats() -> Stats {
    let Ok(conn) = open() else {
        return Stats::default();
    };
    let repos = conn
        .query_row("SELECT COUNT(*) FROM repos", [], |r| r.get::<_, i64>(0))
        .unwrap_or(0) as u64;
    let issues = conn
        .query_row("SELECT COUNT(*) FROM issues", [], |r| r.get::<_, i64>(0))
        .unwrap_or(0) as u64;
    let hints = conn
        .query_row("SELECT COUNT(*) FROM compat_hints", [], |r| r.get::<_, i64>(0))
        .unwrap_or(0) as u64;
    Stats {
        repos,
        issues,
        hints,
    }
}

fn fetch_readme(owner: &str, repo: &str, branch: &str) -> String {
    let branch = if branch.is_empty() { "HEAD" } else { branch };
    for name in ["README.md", "README", "readme.md", "README.MD", "Readme.md"] {
        let url = format!("https://raw.githubusercontent.com/{owner}/{repo}/{branch}/{name}");
        if let Ok(text) = crate::net::get_text(&url) {
            if !text.is_empty() {
                return clamp(text, 80_000);
            }
        }
    }
    String::new()
}

pub fn fetch_repo(owner: &str, repo: &str) -> Result<Repo> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}");
    let v = get_json(&url, &[("Accept", "application/vnd.github+json")])?;
    let license = v
        .get("license")
        .and_then(|l| l.get("spdx_id").or_else(|| l.get("name")))
        .map(|x| x.as_str().unwrap_or("").to_string())
        .filter(|s| !s.is_empty() && s != "NOASSERTION")
        .unwrap_or_default();
    let topics = v
        .get("topics")
        .and_then(|t| t.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    let default_branch = js(v.get("default_branch"));
    let html_url = {
        let s = js(v.get("html_url"));
        if s.is_empty() {
            format!("https://github.com/{owner}/{repo}")
        } else {
            s
        }
    };
    let readme = fetch_readme(owner, repo, &default_branch);
    Ok(Repo {
        owner: owner.to_string(),
        repo: repo.to_string(),
        html_url,
        description: js(v.get("description")),
        stars: ju(v.get("stargazers_count")),
        forks: ju(v.get("forks_count")),
        language: js(v.get("language")),
        license,
        default_branch,
        topics,
        readme,
        open_issues: ju(v.get("open_issues_count")),
        homepage: js(v.get("homepage")),
        pushed_at: js(v.get("pushed_at")),
    })
}

pub fn fetch_issues(owner: &str, repo: &str) -> Vec<GhIssue> {
    let url = format!(
        "https://api.github.com/repos/{owner}/{repo}/issues?state=all&per_page=40&sort=updated"
    );
    let Ok(v) = get_json(&url, &[("Accept", "application/vnd.github+json")]) else {
        return Vec::new();
    };
    let Some(arr) = v.as_array() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for it in arr {
        if it.get("pull_request").is_some() {
            continue;
        }
        let title = js(it.get("title"));
        if title.is_empty() {
            continue;
        }
        let labels = it
            .get("labels")
            .and_then(|l| l.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.get("name").and_then(|n| n.as_str()))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        out.push(GhIssue {
            number: it.get("number").and_then(|n| n.as_i64()).unwrap_or(0),
            title,
            state: js(it.get("state")),
            html_url: js(it.get("html_url")),
            labels,
            body: clamp(js(it.get("body")), 4000),
        });
    }
    out
}

const STOP: &[&str] = &[
    "the", "a", "an", "this", "that", "these", "those", "it", "its", "my", "our", "your",
    "minecraft", "mod", "mods", "version", "versions", "latest", "new", "old", "using", "use",
    "on", "in", "for", "and", "or", "to", "of", "with",
];

fn is_negative(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    [
        "crash",
        "broken",
        "incompatible",
        "not compatible",
        "not work",
        "doesn't work",
        "does not work",
        "won't work",
        "fail",
        "error",
        "conflict",
        "unsupported",
        "can't",
        "cannot",
        "doesnt work",
        "incompat",
    ]
    .iter()
    .any(|k| l.contains(k))
}

fn take_name(s: &str) -> String {
    let mut words = Vec::new();
    for w in s.split_whitespace() {
        let cleaned: String = w
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
            .collect();
        if cleaned.is_empty() {
            break;
        }
        let wl = cleaned.to_ascii_lowercase();
        if STOP.contains(&wl.as_str()) {
            if words.is_empty() {
                continue;
            }
            break;
        }
        words.push(cleaned);
        if words.len() >= 4 {
            break;
        }
    }
    words.join(" ")
}

fn capture_after(hay: &str, needles: &[&str]) -> Vec<String> {
    let lower = hay.to_ascii_lowercase();
    let mut out = Vec::new();
    for n in needles {
        let mut start = 0;
        while let Some(i) = lower[start..].find(n) {
            let abs = start + i + n.len();
            let rest = hay.get(abs..).unwrap_or("").trim_start();
            let rest = rest.trim_start_matches(|c: char| matches!(c, ':' | '-' | ',' | '"' | '\''));
            let target = take_name(rest);
            if target.len() >= 3 {
                out.push(target);
            }
            start = abs.max(start + 1);
        }
    }
    out
}

fn versions_in(s: &str) -> Vec<String> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 3 < b.len() {
        if b[i] == b'1' && b[i + 1] == b'.' && b[i + 2].is_ascii_digit() {
            if i > 0 && b[i - 1].is_ascii_digit() {
                i += 1;
                continue;
            }
            let mut j = i + 2;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if j < b.len() && b[j] == b'.' && j + 1 < b.len() && b[j + 1].is_ascii_digit() {
                j += 1;
                while j < b.len() && b[j].is_ascii_digit() {
                    j += 1;
                }
            }
            if let Ok(v) = std::str::from_utf8(&b[i..j]) {
                if !out.iter().any(|x: &String| x == v) {
                    out.push(v.to_string());
                }
            }
            i = j;
            continue;
        }
        i += 1;
    }
    out
}

pub fn extract_hints(issues: &[GhIssue]) -> Vec<CompatHint> {
    let needles = [
        "incompatible with",
        "not compatible with",
        "conflicts with",
        "conflict with",
        "crashes with",
        "crash with",
        "broken with",
        "doesn't work with",
        "does not work with",
        "won't work with",
        "cannot be used with",
        "can't be used with",
    ];
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for it in issues {
        let blob = format!("{}\n{}\n{}", it.title, it.labels, it.body);
        let negative = is_negative(&blob) || it.state == "open";
        for target in capture_after(&blob, &needles) {
            let key = format!("incompatible|{target}|{}", it.html_url);
            if seen.insert(key) {
                out.push(CompatHint {
                    kind: "incompatible".into(),
                    target,
                    severity: if it.state == "open" { "warn".into() } else { "info".into() },
                    detail: format!("#{} {}", it.number, it.title),
                    issue_url: it.html_url.clone(),
                });
            }
        }
        if negative {
            for ver in versions_in(&blob) {
                let key = format!("version|{ver}|{}", it.html_url);
                if seen.insert(key) {
                    out.push(CompatHint {
                        kind: "version".into(),
                        target: ver,
                        severity: "warn".into(),
                        detail: format!("#{} {}", it.number, it.title),
                        issue_url: it.html_url.clone(),
                    });
                }
            }
            let lower = blob.to_ascii_lowercase();
            for loader in ["neoforge", "fabric", "forge", "quilt"] {
                if lower.contains(loader) {
                    let key = format!("loader|{loader}|{}", it.html_url);
                    if seen.insert(key) {
                        out.push(CompatHint {
                            kind: "loader".into(),
                            target: loader.to_string(),
                            severity: "warn".into(),
                            detail: format!("#{} {}", it.number, it.title),
                            issue_url: it.html_url.clone(),
                        });
                    }
                }
            }
        }
    }
    out
}

pub fn ingest(source: &str, source_id: &str, page: &ModPage, extra_urls: &[&str]) -> Option<Snapshot> {
    let (owner, repo) = find_github(page, extra_urls)?;
    let fetched = fetch_repo(&owner, &repo).ok()?;
    save_repo(source, source_id, &fetched);
    let issues = fetch_issues(&owner, &repo);
    save_issues(source, source_id, &issues);
    let hints = extract_hints(&issues);
    save_hints(source, source_id, &hints);
    Some(Snapshot {
        repo: fetched,
        issues,
        hints,
    })
}

fn names_of(m: &ModHit) -> Vec<String> {
    let mut v = vec![m.name.to_ascii_lowercase(), m.slug.to_ascii_lowercase()];
    v.retain(|s| s.len() >= 3);
    v
}

pub fn pack_hints(mods: &[ModHit], game_version: &str, loader: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let all_names: Vec<(String, Vec<String>)> = mods
        .iter()
        .map(|m| (m.name.clone(), names_of(m)))
        .collect();
    for m in mods {
        for h in load_hints(&m.source, &m.source_id) {
            let t = h.target.to_ascii_lowercase();
            if t.len() < 3 {
                continue;
            }
            match h.kind.as_str() {
                "incompatible" => {
                    for (other, names) in &all_names {
                        if other == &m.name {
                            continue;
                        }
                        if names.iter().any(|n| n == &t || t.contains(n) || n.contains(&t)) {
                            out.push((
                                h.severity.clone(),
                                format!(
                                    "{} may conflict with {other} (GitHub {})",
                                    m.name, h.detail
                                ),
                            ));
                        }
                    }
                }
                "version" if !game_version.is_empty() => {
                    if game_version == h.target
                        || game_version.starts_with(&h.target)
                        || h.target.starts_with(game_version)
                    {
                        out.push((
                            h.severity.clone(),
                            format!(
                                "{} has GitHub issues about Minecraft {}: {}",
                                m.name, h.target, h.detail
                            ),
                        ));
                    }
                }
                "loader" if !loader.is_empty() => {
                    if h.target.eq_ignore_ascii_case(loader) {
                        out.push((
                            h.severity.clone(),
                            format!(
                                "{} has GitHub issues about {loader}: {}",
                                m.name, h.detail
                            ),
                        ));
                    }
                }
                _ => {}
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|(_, msg)| seen.insert(msg.clone()));
    out
}
