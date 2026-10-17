use anyhow::Result;
use rusqlite::{params, Connection};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static WRITING: AtomicBool = AtomicBool::new(false);

use crate::api::ModHit;
use crate::prompt::PromptPlan;
use crate::util;

pub const STALE_SECS: u64 = 20 * 3600;

#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub total: u64,
    pub modrinth: u64,
    pub curseforge: u64,
    pub last_built: String,
    pub last_unix: u64,
}

#[derive(Clone, Debug)]
pub struct ThemeCount {
    pub id: String,
    pub label: String,
    pub count: u64,
}

const THEMES: &[(&str, &str)] = &[
    ("optimization", "Optimization"),
    ("lighting", "Lighting / rendering"),
    ("shaders", "Shaders"),
    ("tech", "Tech"),
    ("magic", "Magic"),
    ("storage", "Storage"),
    ("worldgen", "Worldgen"),
    ("structures", "Structures"),
    ("farming", "Farming"),
    ("decoration", "Decoration"),
    ("combat", "Combat"),
    ("mobs", "Mobs"),
    ("qol", "Quality of life"),
    ("minimap", "Minimap"),
    ("map", "Map"),
    ("recipes", "Recipes"),
    ("inventory", "Inventory"),
    ("create", "Create"),
    ("voice", "Voice"),
    ("render", "Distant render"),
    ("caves", "Caves"),
    ("multiplayer", "Multiplayer"),
];

const CATEGORY_MAP: &[(&str, &str)] = &[
    ("optimization", "optimization"),
    ("performance", "optimization"),
    ("utility", "qol"),
    ("technology", "tech"),
    ("tech", "tech"),
    ("magic", "magic"),
    ("adventure", "structures"),
    ("decoration", "decoration"),
    ("furniture", "decoration"),
    ("food", "farming"),
    ("farming", "farming"),
    ("storage", "storage"),
    ("worldgen", "worldgen"),
    ("world-generation", "worldgen"),
    ("world_generation", "worldgen"),
    ("biomes", "worldgen"),
    ("mobs", "mobs"),
    ("combat", "combat"),
    ("equipment", "combat"),
    ("social", "multiplayer"),
    ("library", "qol"),
    ("game-mechanics", "qol"),
    ("atmosphere", "lighting"),
    ("cursed", "magic"),
    ("economy", "tech"),
    ("transportation", "tech"),
    ("management", "qol"),
    ("kitchen-sink", "tech"),
    ("magic-themed", "magic"),
    ("redstone", "tech"),
    ("energy", "tech"),
    ("player-transport", "tech"),
    ("map-information", "map"),
    ("minimap", "minimap"),
    ("armor-weapons-tools", "combat"),
    ("blood-magic", "magic"),
];

const KEYWORDS: &[(&str, &str)] = &[
    ("sodium", "optimization"),
    ("lithium", "optimization"),
    ("starlight", "optimization"),
    ("ferritecore", "optimization"),
    ("culling", "optimization"),
    ("mekanism", "tech"),
    ("create", "create"),
    ("thermal", "tech"),
    ("immersive", "tech"),
    ("applied energistics", "storage"),
    ("refined storage", "storage"),
    ("ars nouveau", "magic"),
    ("botania", "magic"),
    ("spell", "magic"),
    ("iris", "shaders"),
    ("oculus", "shaders"),
    ("lambdynamic", "lighting"),
    ("dynamic lights", "lighting"),
    ("colorful light", "lighting"),
    ("colourful light", "lighting"),
    ("colorful lightning", "lighting"),
    ("xaero", "minimap"),
    ("journeymap", "map"),
    ("jei", "recipes"),
    ("farmer's delight", "farming"),
    ("voice chat", "voice"),
    ("distant horizons", "render"),
];

pub fn path() -> std::path::PathBuf {
    util::data_dir().join("catalog.db")
}

pub fn open() -> Result<Connection> {
    let conn = Connection::open(path())?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;
         PRAGMA busy_timeout=30000;
         PRAGMA synchronous=NORMAL;
         PRAGMA cache_size=-65536;
         PRAGMA temp_store=MEMORY;",
    )?;
    init(&conn)?;
    Ok(conn)
}

pub fn open_reader() -> Result<Connection> {
    open_reader_ms(30_000)
}

fn open_ui_reader() -> Result<Connection> {
    open_reader_ms(250)
}

fn open_reader_ms(ms: i32) -> Result<Connection> {
    let conn = Connection::open(path())?;
    conn.execute_batch(&format!(
        "PRAGMA query_only=ON;
         PRAGMA busy_timeout={ms};
         PRAGMA temp_store=MEMORY;"
    ))?;
    Ok(conn)
}

pub fn begin_bulk(conn: &Connection) -> Result<()> {
    WRITING.store(true, Ordering::SeqCst);
    conn.execute_batch("PRAGMA cache_size=-131072; PRAGMA temp_store=MEMORY;")?;
    Ok(())
}

pub fn end_bulk(conn: &Connection) -> Result<()> {
    let r = conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE);");
    WRITING.store(false, Ordering::SeqCst);
    r?;
    Ok(())
}

pub fn mark_idle() {
    WRITING.store(false, Ordering::SeqCst);
}

pub fn is_writing() -> bool {
    WRITING.load(Ordering::SeqCst)
}

pub fn reset_index_state(conn: &Connection) -> Result<()> {
    conn.execute("UPDATE index_state SET offset=0, done=0", [])?;
    Ok(())
}

pub fn rebuild_fts(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        INSERT INTO mods_fts(mods_fts) VALUES('delete-all');
        INSERT INTO mods_fts(name, author, slug, summary, source, source_id)
            SELECT name, author, slug, summary, source, source_id FROM mods;
        ",
    )?;
    Ok(())
}

fn init(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS mods (
            source TEXT NOT NULL,
            source_id TEXT NOT NULL,
            slug TEXT,
            name TEXT NOT NULL,
            author TEXT,
            summary TEXT,
            icon_url TEXT,
            website_url TEXT,
            downloads INTEGER DEFAULT 0,
            project_type TEXT,
            date_updated TEXT,
            PRIMARY KEY (source, source_id)
        );
        CREATE TABLE IF NOT EXISTS mod_categories (
            source TEXT NOT NULL,
            source_id TEXT NOT NULL,
            slug TEXT NOT NULL,
            PRIMARY KEY (source, source_id, slug)
        );
        CREATE TABLE IF NOT EXISTS mod_loaders (
            source TEXT NOT NULL,
            source_id TEXT NOT NULL,
            loader TEXT NOT NULL,
            PRIMARY KEY (source, source_id, loader)
        );
        CREATE TABLE IF NOT EXISTS mod_versions (
            source TEXT NOT NULL,
            source_id TEXT NOT NULL,
            game_version TEXT NOT NULL,
            PRIMARY KEY (source, source_id, game_version)
        );
        CREATE TABLE IF NOT EXISTS mod_themes (
            source TEXT NOT NULL,
            source_id TEXT NOT NULL,
            theme_id TEXT NOT NULL,
            PRIMARY KEY (source, source_id, theme_id)
        );
        CREATE TABLE IF NOT EXISTS themes (
            id TEXT PRIMARY KEY,
            label TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS theme_category_map (
            slug TEXT PRIMARY KEY,
            theme_id TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS index_state (
            source TEXT NOT NULL,
            class_or_type TEXT NOT NULL,
            category TEXT NOT NULL,
            offset INTEGER NOT NULL DEFAULT 0,
            done INTEGER NOT NULL DEFAULT 0,
            updated_at INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (source, class_or_type, category)
        );
        CREATE TABLE IF NOT EXISTS meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE VIRTUAL TABLE IF NOT EXISTS mods_fts USING fts5(
            name, author, slug, summary, source UNINDEXED, source_id UNINDEXED
        );
        CREATE INDEX IF NOT EXISTS idx_mods_downloads ON mods(downloads);
        CREATE INDEX IF NOT EXISTS idx_mod_themes ON mod_themes(theme_id);
        ",
    )?;
    for (id, label) in THEMES {
        conn.execute(
            "INSERT OR IGNORE INTO themes (id, label) VALUES (?1, ?2)",
            params![id, label],
        )?;
    }
    for (slug, theme) in CATEGORY_MAP {
        conn.execute(
            "INSERT OR IGNORE INTO theme_category_map (slug, theme_id) VALUES (?1, ?2)",
            params![slug, theme],
        )?;
    }
    Ok(())
}

pub fn stats() -> Stats {
    catch_unwind(AssertUnwindSafe(|| {
        let conn = open_ui_reader().ok()?;
        let total = conn
            .query_row("SELECT COUNT(*) FROM mods", [], |r| r.get::<_, i64>(0))
            .unwrap_or(0) as u64;
        let modrinth = conn
            .query_row(
                "SELECT COUNT(*) FROM mods WHERE source='modrinth'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0) as u64;
        let curseforge = conn
            .query_row(
                "SELECT COUNT(*) FROM mods WHERE source='curseforge'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0) as u64;
        let last_unix: u64 = conn
            .query_row(
                "SELECT value FROM meta WHERE key='last_index_completed'",
                [],
                |r| r.get::<_, String>(0),
            )
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let last_built = if last_unix == 0 {
            "never".into()
        } else {
            format!("{last_unix}")
        };
        Some(Stats {
            total,
            modrinth,
            curseforge,
            last_built,
            last_unix,
        })
    }))
    .ok()
    .flatten()
    .unwrap_or_default()
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn should_auto_index() -> bool {
    let s = stats();
    s.total == 0 || s.last_unix == 0
}

pub fn can_refresh(force: bool) -> bool {
    if force {
        return true;
    }
    let s = stats();
    if s.total == 0 || s.last_unix == 0 {
        return true;
    }
    now_unix().saturating_sub(s.last_unix) >= STALE_SECS
}

pub fn mark_completed(conn: &Connection) -> Result<()> {
    conn.execute(
        "INSERT INTO meta(key, value) VALUES ('last_index_completed', ?1)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![now_unix().to_string()],
    )?;
    Ok(())
}

pub fn get_state(conn: &Connection, source: &str, class_or_type: &str, category: &str) -> (u32, bool) {
    conn.query_row(
        "SELECT offset, done FROM index_state WHERE source=?1 AND class_or_type=?2 AND category=?3",
        params![source, class_or_type, category],
        |r| Ok((r.get::<_, i64>(0)? as u32, r.get::<_, i64>(1)? != 0)),
    )
    .unwrap_or((0, false))
}

pub fn set_state(
    conn: &Connection,
    source: &str,
    class_or_type: &str,
    category: &str,
    offset: u32,
    done: bool,
) -> Result<()> {
    conn.execute(
        "INSERT INTO index_state(source, class_or_type, category, offset, done, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(source, class_or_type, category) DO UPDATE SET
            offset=excluded.offset, done=excluded.done, updated_at=excluded.updated_at",
        params![
            source,
            class_or_type,
            category,
            offset as i64,
            if done { 1 } else { 0 },
            now_unix() as i64
        ],
    )?;
    Ok(())
}

pub fn upsert_mods(conn: &Connection, hits: &[ModHit]) -> Result<()> {
    if hits.is_empty() {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    {
        let mut ins_mod = tx.prepare(
            "INSERT INTO mods(source, source_id, slug, name, author, summary, icon_url, website_url, downloads, project_type)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(source, source_id) DO UPDATE SET
                slug=excluded.slug, name=excluded.name, author=excluded.author, summary=excluded.summary,
                icon_url=excluded.icon_url, website_url=excluded.website_url, downloads=excluded.downloads,
                project_type=excluded.project_type",
        )?;
        let mut del_cat = tx.prepare("DELETE FROM mod_categories WHERE source=?1 AND source_id=?2")?;
        let mut ins_cat = tx.prepare(
            "INSERT OR IGNORE INTO mod_categories(source, source_id, slug) VALUES (?1, ?2, ?3)",
        )?;
        let mut del_load = tx.prepare("DELETE FROM mod_loaders WHERE source=?1 AND source_id=?2")?;
        let mut ins_load = tx
            .prepare("INSERT OR IGNORE INTO mod_loaders(source, source_id, loader) VALUES (?1, ?2, ?3)")?;
        let mut del_ver = tx.prepare("DELETE FROM mod_versions WHERE source=?1 AND source_id=?2")?;
        let mut ins_ver = tx.prepare(
            "INSERT OR IGNORE INTO mod_versions(source, source_id, game_version) VALUES (?1, ?2, ?3)",
        )?;
        let mut del_theme = tx.prepare("DELETE FROM mod_themes WHERE source=?1 AND source_id=?2")?;
        let mut ins_theme = tx
            .prepare("INSERT OR IGNORE INTO mod_themes(source, source_id, theme_id) VALUES (?1, ?2, ?3)")?;
        let mut del_fts = tx.prepare("DELETE FROM mods_fts WHERE source=?1 AND source_id=?2")?;
        let mut ins_fts = tx.prepare(
            "INSERT INTO mods_fts(name, author, slug, summary, source, source_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        for m in hits {
            let ptype = m
                .categories
                .first()
                .cloned()
                .unwrap_or_else(|| "mod".into());
            ins_mod.execute(params![
                m.source,
                m.source_id,
                m.slug,
                m.name,
                m.author,
                m.summary,
                m.icon_url,
                m.website_url,
                m.downloads as i64,
                ptype
            ])?;
            del_cat.execute(params![m.source, m.source_id])?;
            del_load.execute(params![m.source, m.source_id])?;
            del_ver.execute(params![m.source, m.source_id])?;
            del_theme.execute(params![m.source, m.source_id])?;
            for c in &m.categories {
                if !c.is_empty() {
                    ins_cat.execute(params![m.source, m.source_id, c.to_ascii_lowercase()])?;
                }
            }
            for l in &m.loaders {
                if !l.is_empty() {
                    ins_load.execute(params![m.source, m.source_id, l.to_ascii_lowercase()])?;
                }
            }
            for v in m.game_versions.iter().take(80) {
                if !v.is_empty() {
                    ins_ver.execute(params![m.source, m.source_id, v])?;
                }
            }
            for theme in themes_for(m) {
                ins_theme.execute(params![m.source, m.source_id, theme])?;
            }
            del_fts.execute(params![m.source, m.source_id])?;
            ins_fts.execute(params![
                m.name,
                m.author,
                m.slug,
                m.summary,
                m.source,
                m.source_id
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn themes_for(m: &ModHit) -> Vec<String> {
    let mut out = Vec::new();
    for c in &m.categories {
        let slug = c.to_ascii_lowercase();
        if let Some((_, theme)) = CATEGORY_MAP.iter().find(|(s, _)| *s == slug) {
            if !out.iter().any(|t| t == theme) {
                out.push((*theme).to_string());
            }
        }
    }
    let blob = format!("{} {}", m.name, m.summary).to_ascii_lowercase();
    for (key, theme) in KEYWORDS {
        if blob.contains(key) && !out.iter().any(|t| t == theme) {
            out.push((*theme).to_string());
        }
    }
    out
}

fn row_to_hit(
    source: String,
    source_id: String,
    slug: String,
    name: String,
    author: String,
    summary: String,
    icon_url: String,
    website_url: String,
    downloads: i64,
    project_type: String,
) -> ModHit {
    ModHit {
        source,
        source_id,
        slug,
        name,
        author,
        summary,
        icon_url,
        website_url,
        categories: vec![project_type],
        loaders: Vec::new(),
        game_versions: Vec::new(),
        downloads: downloads as u64,
    }
}

fn list_col(conn: &Connection, sql: &str, source: &str, source_id: &str) -> Vec<String> {
    let Ok(mut stmt) = conn.prepare(sql) else {
        return Vec::new();
    };
    stmt.query_map(params![source, source_id], |r| r.get::<_, String>(0))
        .map(|rows| rows.filter_map(|x| x.ok()).collect())
        .unwrap_or_default()
}

fn hydrate(conn: &Connection, hits: &mut [ModHit]) {
    if WRITING.load(Ordering::SeqCst) {
        return;
    }
    for m in hits {
        m.loaders = list_col(
            conn,
            "SELECT loader FROM mod_loaders WHERE source=?1 AND source_id=?2",
            &m.source,
            &m.source_id,
        );
        m.game_versions = list_col(
            conn,
            "SELECT game_version FROM mod_versions WHERE source=?1 AND source_id=?2 LIMIT 24",
            &m.source,
            &m.source_id,
        );
        let cats = list_col(
            conn,
            "SELECT slug FROM mod_categories WHERE source=?1 AND source_id=?2",
            &m.source,
            &m.source_id,
        );
        if !cats.is_empty() {
            m.categories = cats;
        }
    }
}

fn apply_filters(sql: &mut String, params: &mut Vec<String>, source: &str, loader: &str, version: &str, ptype: &str) {
    if source != "all" && source != "local" && !source.is_empty() {
        sql.push_str(" AND m.source = ?");
        params.push(source.to_string());
    }
    if !ptype.is_empty() {
        sql.push_str(" AND m.project_type = ?");
        params.push(ptype.to_string());
    }
    if !loader.is_empty() {
        sql.push_str(" AND EXISTS (SELECT 1 FROM mod_loaders l WHERE l.source=m.source AND l.source_id=m.source_id AND l.loader = ?)");
        params.push(loader.to_ascii_lowercase());
    }
    if !version.is_empty() {
        sql.push_str(" AND EXISTS (SELECT 1 FROM mod_versions v WHERE v.source=m.source AND v.source_id=m.source_id AND v.game_version = ?)");
        params.push(version.to_string());
    }
}

fn query_hits(conn: &Connection, sql: &str, bind: &[String]) -> Vec<ModHit> {
    let Ok(mut stmt) = conn.prepare(sql) else {
        return Vec::new();
    };
    let refs: Vec<&str> = bind.iter().map(|s| s.as_str()).collect();
    let rows = stmt.query_map(rusqlite::params_from_iter(refs), |r| {
        Ok(row_to_hit(
            r.get(0)?,
            r.get(1)?,
            r.get(2)?,
            r.get(3)?,
            r.get(4)?,
            r.get(5)?,
            r.get(6)?,
            r.get(7)?,
            r.get(8)?,
            r.get(9)?,
        ))
    });
    let mut hits = match rows {
        Ok(it) => it.filter_map(|x| x.ok()).collect::<Vec<_>>(),
        Err(_) => return Vec::new(),
    };
    hydrate(conn, &mut hits);
    hits
}

pub fn theme_counts() -> Vec<ThemeCount> {
    catch_unwind(AssertUnwindSafe(|| {
        let conn = open_ui_reader().ok()?;
        let mut stmt = conn
            .prepare(
                "SELECT t.id, t.label, COUNT(mt.source_id) FROM themes t
                 LEFT JOIN mod_themes mt ON mt.theme_id = t.id
                 GROUP BY t.id, t.label
                 ORDER BY COUNT(mt.source_id) DESC, t.label",
            )
            .ok()?;
        stmt.query_map([], |r| {
            Ok(ThemeCount {
                id: r.get(0)?,
                label: r.get(1)?,
                count: r.get::<_, i64>(2)? as u64,
            })
        })
        .ok()
        .map(|rows| rows.filter_map(|x| x.ok()).collect())
    }))
    .ok()
    .flatten()
    .unwrap_or_default()
}

pub fn mods_for_theme(
    theme: &str,
    source: &str,
    loader: &str,
    version: &str,
    project_type: &str,
    limit: u32,
) -> Vec<ModHit> {
    catch_unwind(AssertUnwindSafe(|| {
        let conn = open_ui_reader().ok()?;
        let mut sql = String::from(
            "SELECT m.source, m.source_id, m.slug, m.name, m.author, m.summary, m.icon_url, m.website_url, m.downloads, m.project_type
             FROM mods m",
        );
        let mut bind = Vec::new();
        if !theme.is_empty() {
            sql.push_str(" INNER JOIN mod_themes mt ON mt.source=m.source AND mt.source_id=m.source_id AND mt.theme_id=?");
            bind.push(theme.to_string());
        }
        sql.push_str(" WHERE 1=1");
        apply_filters(&mut sql, &mut bind, source, loader, version, project_type);
        sql.push_str(" ORDER BY m.downloads DESC LIMIT ?");
        bind.push(limit.to_string());
        Some(query_hits(&conn, &sql, &bind))
    }))
    .ok()
    .flatten()
    .unwrap_or_default()
}

fn fts_match(q: &str) -> Option<String> {
    let tokens: Vec<String> = q
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| t.len() >= 2)
        .map(|t| {
            let clean: String = t.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
            format!("{clean}*")
        })
        .filter(|t| t.len() > 1)
        .collect();
    if tokens.is_empty() {
        None
    } else {
        Some(tokens.join(" AND "))
    }
}

fn like_search(
    conn: &Connection,
    query: &str,
    source: &str,
    loader: &str,
    version: &str,
    project_type: &str,
    limit: u32,
) -> Vec<ModHit> {
    let clean: String = query
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || c.is_ascii_whitespace())
        .collect();
    let needle = clean.split_whitespace().next().unwrap_or("").to_string();
    if needle.len() < 2 {
        return Vec::new();
    }
    let pat = format!("%{needle}%");
    let mut sql = String::from(
        "SELECT m.source, m.source_id, m.slug, m.name, m.author, m.summary, m.icon_url, m.website_url, m.downloads, m.project_type
         FROM mods m
         WHERE (m.name LIKE ? OR m.slug LIKE ? OR m.author LIKE ? OR m.summary LIKE ?)",
    );
    let mut bind = vec![pat.clone(), pat.clone(), pat.clone(), pat];
    apply_filters(&mut sql, &mut bind, source, loader, version, project_type);
    sql.push_str(" ORDER BY m.downloads DESC LIMIT ?");
    bind.push(limit.to_string());
    query_hits(conn, &sql, &bind)
}

pub fn fts_search(
    query: &str,
    source: &str,
    loader: &str,
    version: &str,
    project_type: &str,
    limit: u32,
) -> Vec<ModHit> {
    catch_unwind(AssertUnwindSafe(|| {
        let conn = open_ui_reader().ok()?;
        if WRITING.load(Ordering::SeqCst) {
            return Some(like_search(
                &conn,
                query,
                source,
                loader,
                version,
                project_type,
                limit,
            ));
        }
        let mq = fts_match(query)?;
        let mut sql = String::from(
            "SELECT m.source, m.source_id, m.slug, m.name, m.author, m.summary, m.icon_url, m.website_url, m.downloads, m.project_type
             FROM mods m
             INNER JOIN mods_fts f ON f.source=m.source AND f.source_id=m.source_id
             WHERE f MATCH ?",
        );
        let mut bind = vec![mq];
        apply_filters(&mut sql, &mut bind, source, loader, version, project_type);
        sql.push_str(" ORDER BY m.downloads DESC LIMIT ?");
        bind.push(limit.to_string());
        let hits = query_hits(&conn, &sql, &bind);
        if hits.is_empty() {
            Some(like_search(
                &conn,
                query,
                source,
                loader,
                version,
                project_type,
                limit,
            ))
        } else {
            Some(hits)
        }
    }))
    .ok()
    .flatten()
    .unwrap_or_default()
}

pub fn prompt_search(
    plan: &PromptPlan,
    source: &str,
    loader: &str,
    version: &str,
    project_type: &str,
) -> Vec<ModHit> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    if !plan.raw.is_empty() {
        for m in fts_search(&plan.raw, source, loader, version, project_type, 40) {
            if seen.insert(format!("{}:{}", m.source, m.source_id)) {
                out.push(m);
            }
        }
    }
    for theme in &plan.themes {
        for m in mods_for_theme(theme, source, loader, version, project_type, 40) {
            if seen.insert(format!("{}:{}", m.source, m.source_id)) {
                out.push(m);
            }
        }
    }
    for q in plan.queries.iter().chain(plan.tokens.iter()) {
        for m in fts_search(q, source, loader, version, project_type, 20) {
            if seen.insert(format!("{}:{}", m.source, m.source_id)) {
                out.push(m);
            }
        }
    }
    out.sort_by(|a, b| b.downloads.cmp(&a.downloads));
    out.truncate(50);
    out
}


