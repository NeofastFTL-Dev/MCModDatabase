use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::net::{agent, describe_err, get_json};

const MR: &str = "https://api.modrinth.com/v2";
const CF: &str = "https://api.curseforge.com/v1";

const LOADERS: &[&str] = &[
    "fabric", "forge", "neoforge", "quilt", "liteloader", "rift", "babric",
];

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ModPage {
    pub body: String,
    pub source_url: String,
    pub issues_url: String,
    pub wiki_url: String,
    pub discord_url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModHit {
    pub source: String,
    pub source_id: String,
    pub slug: String,
    pub name: String,
    pub author: String,
    pub summary: String,
    pub icon_url: String,
    pub website_url: String,
    pub categories: Vec<String>,
    pub loaders: Vec<String>,
    pub game_versions: Vec<String>,
    pub downloads: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModFile {
    pub source: String,
    pub source_id: String,
    pub name: String,
    pub slug: String,
    pub file_name: String,
    pub url: String,
    pub loaders: Vec<String>,
    pub game_versions: Vec<String>,
    pub dependencies: Vec<Dep>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Dep {
    pub project_id: String,
    pub kind: String,
    pub name: String,
    pub slug: String,
}

pub fn key(m: &ModHit) -> String {
    format!("{}:{}", m.source, m.source_id)
}

fn js(v: &Value) -> String {
    v.as_str().unwrap_or("").to_string()
}

fn ju(v: &Value) -> u64 {
    v.as_u64()
        .or_else(|| v.as_f64().map(|x| x as u64))
        .unwrap_or(0)
}

fn split_cats(cats: &[Value]) -> (Vec<String>, Vec<String>) {
    let mut out = Vec::new();
    let mut loaders = Vec::new();
    for c in cats {
        let s = js(c);
        if LOADERS.contains(&s.to_ascii_lowercase().as_str()) {
            loaders.push(s.to_ascii_lowercase());
        } else if !s.is_empty() {
            out.push(s);
        }
    }
    (out, loaders)
}

fn cf_headers(key: &str) -> Vec<(&str, &str)> {
    vec![("Accept", "application/json"), ("x-api-key", key)]
}

pub fn cf_class(project_type: &str) -> i32 {
    match project_type {
        "modpack" => 4471,
        "resourcepack" => 12,
        "shader" => 6552,
        _ => 6,
    }
}

fn cf_loader(loader: &str) -> Option<i32> {
    match loader {
        "forge" => Some(1),
        "fabric" => Some(4),
        "quilt" => Some(5),
        "neoforge" => Some(6),
        _ => None,
    }
}

fn mr_sort(sort: &str) -> &'static str {
    match sort {
        "downloads" => "downloads",
        "follows" => "follows",
        "newest" => "newest",
        "updated" => "updated",
        _ => "relevance",
    }
}

fn cf_sort(sort: &str) -> i32 {
    match sort {
        "follows" => 2,
        "updated" => 3,
        "name" => 4,
        "downloads" => 6,
        "newest" => 11,
        _ => 1,
    }
}

pub fn normalize_modrinth(hit: &Value) -> ModHit {
    let cats = hit
        .get("categories")
        .or_else(|| hit.get("display_categories"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let (mut categories, loaders) = split_cats(&cats);
    let ptype = js(hit.get("project_type").unwrap_or(&Value::Null));
    let ptype = if ptype.is_empty() {
        "mod".into()
    } else {
        ptype
    };
    categories.insert(0, ptype.clone());
    let id = hit
        .get("project_id")
        .or_else(|| hit.get("id"))
        .map(js)
        .unwrap_or_default();
    let slug = {
        let s = js(hit.get("slug").unwrap_or(&Value::Null));
        if s.is_empty() {
            id.clone()
        } else {
            s
        }
    };
    let path = match ptype.as_str() {
        "modpack" => "modpack",
        "resourcepack" => "resourcepack",
        "shader" => "shader",
        "datapack" => "datapack",
        _ => "mod",
    };
    let versions = hit
        .get("versions")
        .or_else(|| hit.get("game_versions"))
        .and_then(|v| v.as_array())
        .map(|a| a.iter().map(js).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();
    ModHit {
        source: "modrinth".into(),
        source_id: id,
        slug: slug.clone(),
        name: js(hit.get("title").or_else(|| hit.get("name")).unwrap_or(&Value::Null)),
        author: js(hit.get("author").unwrap_or(&Value::Null)),
        summary: js(hit.get("description").unwrap_or(&Value::Null)),
        icon_url: js(hit.get("icon_url").unwrap_or(&Value::Null)),
        website_url: format!("https://modrinth.com/{path}/{slug}"),
        categories,
        loaders,
        game_versions: versions,
        downloads: ju(hit.get("downloads").unwrap_or(&Value::Null)),
    }
}

pub fn normalize_curseforge(mod_: &Value) -> ModHit {
    let authors = mod_.get("authors").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let author = authors
        .first()
        .and_then(|a| a.get("name"))
        .map(js)
        .unwrap_or_default();
    let logo = mod_.get("logo").cloned().unwrap_or(Value::Null);
    let class_id = mod_.get("classId").and_then(|v| v.as_i64()).unwrap_or(6);
    let ptype = match class_id {
        4471 => "modpack",
        12 => "resourcepack",
        6552 => "shader",
        _ => "mod",
    };
    let slug = {
        let s = js(mod_.get("slug").unwrap_or(&Value::Null));
        if s.is_empty() {
            ju(mod_.get("id").unwrap_or(&Value::Null)).to_string()
        } else {
            s
        }
    };
    let path = match ptype {
        "modpack" => "modpacks",
        "resourcepack" => "texture-packs",
        "shader" => "shaders",
        _ => "mc-mods",
    };
    let website = mod_
        .get("links")
        .and_then(|l| l.get("websiteUrl"))
        .map(js)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("https://www.curseforge.com/minecraft/{path}/{slug}"));
    let mut loaders = Vec::new();
    let mut versions = Vec::new();
    if let Some(idx) = mod_.get("latestFilesIndexes").and_then(|v| v.as_array()) {
        for item in idx {
            if let Some(ml) = item.get("modLoader").and_then(|v| v.as_i64()) {
                let name = match ml {
                    1 => "forge",
                    4 => "fabric",
                    5 => "quilt",
                    6 => "neoforge",
                    _ => "",
                };
                if !name.is_empty() && !loaders.iter().any(|x| x == name) {
                    loaders.push(name.to_string());
                }
            }
            if let Some(v) = item.get("gameVersion").map(js) {
                if !v.is_empty() && !versions.iter().any(|x| x == &v) {
                    versions.push(v);
                }
            }
        }
    }
    let mut categories = vec![ptype.to_string()];
    if let Some(cats) = mod_.get("categories").and_then(|v| v.as_array()) {
        for c in cats {
            let s = c.get("slug").or_else(|| c.get("name")).map(js).unwrap_or_default();
            if !s.is_empty() {
                categories.push(s);
            }
        }
    }
    ModHit {
        source: "curseforge".into(),
        source_id: ju(mod_.get("id").unwrap_or(&Value::Null)).to_string(),
        slug,
        name: js(mod_.get("name").unwrap_or(&Value::Null)),
        author,
        summary: js(mod_.get("summary").unwrap_or(&Value::Null)),
        icon_url: js(logo.get("thumbnailUrl").or_else(|| logo.get("url")).unwrap_or(&Value::Null)),
        website_url: website,
        categories,
        loaders,
        game_versions: versions,
        downloads: ju(mod_.get("downloadCount").unwrap_or(&Value::Null)),
    }
}

pub fn search_modrinth(
    query: &str,
    project_type: &str,
    loader: &str,
    version: &str,
    sort: &str,
    offset: u32,
    limit: u32,
) -> Result<(Vec<ModHit>, u64)> {
    let mut facets: Vec<Vec<String>> = Vec::new();
    if !project_type.is_empty() {
        facets.push(vec![format!("project_type:{project_type}")]);
    }
    if !loader.is_empty() {
        facets.push(vec![format!("categories:{loader}")]);
    }
    if !version.is_empty() {
        facets.push(vec![format!("versions:{version}")]);
    }
    let mut req = agent()
        .get(&format!("{MR}/search"))
        .query("query", query)
        .query("index", mr_sort(sort))
        .query("offset", &offset.to_string())
        .query("limit", &limit.min(100).to_string());
    if !facets.is_empty() {
        req = req.query("facets", &serde_json::to_string(&facets)?);
    }
    let payload: Value = req
        .call()
        .map_err(|e| anyhow!("{}", describe_err(e)))?
        .into_json()?;
    let hits = payload
        .get("hits")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().map(normalize_modrinth).collect())
        .unwrap_or_default();
    let total = ju(payload.get("total_hits").unwrap_or(&Value::Null));
    Ok((hits, total))
}

pub fn search_curseforge(
    api_key: &str,
    query: &str,
    project_type: &str,
    loader: &str,
    version: &str,
    sort: &str,
    offset: u32,
    limit: u32,
) -> Result<(Vec<ModHit>, u64)> {
    let mut req = agent()
        .get(&format!("{CF}/mods/search"))
        .set("Accept", "application/json")
        .set("x-api-key", api_key)
        .query("gameId", "432")
        .query("classId", &cf_class(project_type).to_string())
        .query("searchFilter", query)
        .query("sortField", &cf_sort(sort).to_string())
        .query("sortOrder", if sort == "name" { "asc" } else { "desc" })
        .query("pageSize", &limit.min(50).to_string())
        .query("index", &offset.to_string());
    if !version.is_empty() {
        req = req.query("gameVersion", version);
    }
    if let Some(ml) = cf_loader(loader) {
        req = req.query("modLoaderType", &ml.to_string());
    }
    let payload: Value = req
        .call()
        .map_err(|e| anyhow!("{}", describe_err(e)))?
        .into_json()?;
    let hits: Vec<ModHit> = payload
        .get("data")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().map(normalize_curseforge).collect())
        .unwrap_or_default();
    let total = payload
        .get("pagination")
        .and_then(|p| p.get("totalCount"))
        .map(ju)
        .unwrap_or(hits.len() as u64);
    Ok((hits, total))
}

pub fn search(
    query: &str,
    source: &str,
    project_type: &str,
    loader: &str,
    version: &str,
    sort: &str,
    offset: u32,
    limit: u32,
    cf_key: &str,
) -> (Vec<ModHit>, u64, Vec<String>) {
    let mut warnings = Vec::new();
    let mut groups: Vec<Vec<ModHit>> = Vec::new();
    let mut totals = 0u64;
    if source == "all" || source == "modrinth" {
        match search_modrinth(query, project_type, loader, version, sort, offset, limit) {
            Ok((hits, total)) => {
                totals += total;
                groups.push(hits);
            }
            Err(e) => warnings.push(format!("Modrinth: {e}")),
        }
    }
    if source == "all" || source == "curseforge" {
        if cf_key.trim().is_empty() {
            warnings.push("CurseForge skipped - add an API key in Settings.".into());
        } else {
            match search_curseforge(
                cf_key,
                query,
                project_type,
                loader,
                version,
                sort,
                offset,
                limit,
            ) {
                Ok((hits, total)) => {
                    totals += total;
                    groups.push(hits);
                }
                Err(e) => warnings.push(format!("CurseForge: {e}")),
            }
        }
    }
    let mut merged = Vec::new();
    let mut i = 0usize;
    loop {
        let mut any = false;
        for g in &groups {
            if let Some(item) = g.get(i) {
                merged.push(item.clone());
                any = true;
            }
        }
        if !any {
            break;
        }
        i += 1;
    }
    (merged, totals, warnings)
}

#[derive(Clone, Debug)]
pub struct Category {
    pub slug: String,
    pub name: String,
    pub id: Option<i32>,
}

pub const PROJECT_TYPES: &[&str] = &["mod", "modpack", "resourcepack", "shader"];

pub fn list_modrinth_categories() -> Result<Vec<Category>> {
    let payload = get_json(&format!("{MR}/tag/category"), &[])?;
    Ok(payload
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| {
                    let name = js(v.get("name").unwrap_or(&Value::Null));
                    if name.is_empty() {
                        return None;
                    }
                    Some(Category {
                        slug: name.clone(),
                        name,
                        id: None,
                    })
                })
                .collect()
        })
        .unwrap_or_default())
}

pub fn list_curseforge_categories(api_key: &str, class_id: i32) -> Result<Vec<Category>> {
    let payload = get_json(
        &format!("{CF}/categories?gameId=432&classId={class_id}"),
        &cf_headers(api_key),
    )?;
    Ok(payload
        .get("data")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| {
                    let id = v.get("id").and_then(|x| x.as_i64()).map(|x| x as i32)?;
                    let slug = js(v.get("slug").unwrap_or(&Value::Null));
                    let name = js(v.get("name").unwrap_or(&Value::Null));
                    if slug.is_empty() && name.is_empty() {
                        return None;
                    }
                    Some(Category {
                        slug: if slug.is_empty() {
                            name.to_ascii_lowercase()
                        } else {
                            slug
                        },
                        name,
                        id: Some(id),
                    })
                })
                .collect()
        })
        .unwrap_or_default())
}

pub fn browse_modrinth(project_type: &str, offset: u32, limit: u32) -> Result<(Vec<ModHit>, u64)> {
    search_modrinth("", project_type, "", "", "downloads", offset, limit)
}

pub fn browse_curseforge(
    api_key: &str,
    project_type: &str,
    category_id: Option<i32>,
    offset: u32,
    limit: u32,
) -> Result<(Vec<ModHit>, u64)> {
    let mut req = agent()
        .get(&format!("{CF}/mods/search"))
        .set("Accept", "application/json")
        .set("x-api-key", api_key)
        .query("gameId", "432")
        .query("classId", &cf_class(project_type).to_string())
        .query("sortField", "6")
        .query("sortOrder", "desc")
        .query("pageSize", &limit.min(50).to_string())
        .query("index", &offset.to_string());
    if let Some(id) = category_id {
        req = req.query("categoryId", &id.to_string());
    }
    let payload: Value = req
        .call()
        .map_err(|e| anyhow!("{}", describe_err(e)))?
        .into_json()?;
    let hits: Vec<ModHit> = payload
        .get("data")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().map(normalize_curseforge).collect())
        .unwrap_or_default();
    let total = payload
        .get("pagination")
        .and_then(|p| p.get("totalCount"))
        .map(ju)
        .unwrap_or(hits.len() as u64);
    Ok((hits, total))
}

pub fn game_versions() -> Result<Vec<String>> {
    let payload = get_json(&format!("{MR}/tag/game_version"), &[])?;
    Ok(payload
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|v| v.get("version_type").map(js).as_deref() == Some("release"))
                .filter_map(|v| v.get("version").map(js))
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default())
}

pub fn loader_versions(loader: &str, game_version: &str) -> Vec<String> {
    let mut versions = Vec::new();
    match loader {
        "fabric" => {
            let url = if game_version.is_empty() {
                "https://meta.fabricmc.net/v2/versions/loader".into()
            } else {
                format!("https://meta.fabricmc.net/v2/versions/loader/{game_version}")
            };
            if let Ok(payload) = get_json(&url, &[]) {
                if let Some(arr) = payload.as_array() {
                    for item in arr {
                        let ver = item
                            .get("loader")
                            .and_then(|l| l.get("version"))
                            .or_else(|| item.get("version"))
                            .map(js)
                            .unwrap_or_default();
                        if !ver.is_empty() {
                            versions.push(ver);
                        }
                    }
                }
            }
        }
        "quilt" => {
            if let Ok(payload) = get_json("https://meta.quiltmc.org/v3/versions/loader", &[]) {
                if let Some(arr) = payload.as_array() {
                    for item in arr {
                        let ver = js(item.get("version").unwrap_or(&Value::Null));
                        if !ver.is_empty() {
                            versions.push(ver);
                        }
                    }
                }
            }
        }
        "forge" => {
            if let Ok(promo) = get_json(
                "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json",
                &[],
            ) {
                if let Some(obj) = promo.get("promos").and_then(|v| v.as_object()) {
                    for (k, v) in obj {
                        if !game_version.is_empty() && !k.starts_with(game_version) {
                            continue;
                        }
                        let ver = js(v);
                        if !ver.is_empty() && !versions.contains(&ver) {
                            versions.push(ver);
                        }
                    }
                }
            }
        }
        "neoforge" => {
            if let Ok(text) = crate::net::get_text(
                "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml",
            ) {
                let prefix = if game_version.is_empty() {
                    String::new()
                } else {
                    let parts: Vec<_> = game_version.split('.').collect();
                    if parts.len() >= 3 {
                        format!("{}.{}", parts[1], parts[2])
                    } else if parts.len() == 2 {
                        format!("{}.0", parts[1])
                    } else {
                        String::new()
                    }
                };
                for chunk in text.split("<version>").skip(1) {
                    if let Some(end) = chunk.find("</version>") {
                        let ver = chunk[..end].trim();
                        if !ver.is_empty() && (prefix.is_empty() || ver.starts_with(&prefix)) {
                            versions.push(ver.to_string());
                        }
                    }
                }
                versions.reverse();
            }
        }
        _ => {}
    }
    versions.truncate(80);
    versions
}

fn cf_dep_kind(n: i64) -> &'static str {
    match n {
        3 => "required",
        5 => "incompatible",
        1 | 6 => "embedded",
        _ => "optional",
    }
}

pub fn resolve_file(
    hit: &ModHit,
    game_version: &str,
    loader: &str,
    cf_key: &str,
) -> Result<Option<ModFile>> {
    if hit.source == "modrinth" {
        let mut req = agent().get(&format!("{MR}/project/{}/version", hit.source_id));
        if !game_version.is_empty() {
            req = req.query("game_versions", &serde_json::to_string(&[game_version])?);
        }
        if !loader.is_empty() {
            req = req.query("loaders", &serde_json::to_string(&[loader])?);
        }
        let payload: Value = req
            .call()
            .map_err(|e| anyhow!("{}", describe_err(e)))?
            .into_json()?;
        let Some(arr) = payload.as_array() else {
            return Ok(None);
        };
        let Some(ver) = arr.first() else {
            return Ok(None);
        };
        let files = ver.get("files").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let primary = files
            .iter()
            .find(|f| f.get("primary").and_then(|v| v.as_bool()) == Some(true))
            .or(files.first());
        let url = primary
            .and_then(|f| f.get("url"))
            .map(js)
            .unwrap_or_default();
        let file_name = primary
            .and_then(|f| f.get("filename"))
            .map(js)
            .unwrap_or_else(|| js(ver.get("name").unwrap_or(&Value::Null)));
        let deps = ver
            .get("dependencies")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .map(|d| Dep {
                        project_id: js(d.get("project_id").unwrap_or(&Value::Null)),
                        kind: js(d.get("dependency_type").unwrap_or(&Value::Null)),
                        name: String::new(),
                        slug: String::new(),
                    })
                    .filter(|d| !d.project_id.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let loaders = ver
            .get("loaders")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().map(js).collect())
            .unwrap_or_default();
        let game_versions = ver
            .get("game_versions")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().map(js).collect())
            .unwrap_or_default();
        return Ok(Some(ModFile {
            source: hit.source.clone(),
            source_id: hit.source_id.clone(),
            name: hit.name.clone(),
            slug: hit.slug.clone(),
            file_name,
            url,
            loaders,
            game_versions,
            dependencies: deps,
        }));
    }
    if hit.source == "curseforge" {
        if cf_key.trim().is_empty() {
            return Err(anyhow!("CurseForge API key required"));
        }
        let mut req = agent()
            .get(&format!("{CF}/mods/{}/files", hit.source_id))
            .set("Accept", "application/json")
            .set("x-api-key", cf_key)
            .query("pageSize", "50");
        if !game_version.is_empty() {
            req = req.query("gameVersion", game_version);
        }
        if let Some(ml) = cf_loader(loader) {
            req = req.query("modLoaderType", &ml.to_string());
        }
        let payload: Value = req
            .call()
            .map_err(|e| anyhow!("{}", describe_err(e)))?
            .into_json()?;
        let Some(file) = payload
            .get("data")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
        else {
            return Ok(None);
        };
        let gvs = file
            .get("gameVersions")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let mut loaders = Vec::new();
        let mut versions = Vec::new();
        for v in &gvs {
            let s = js(v);
            if LOADERS.contains(&s.to_ascii_lowercase().as_str()) {
                loaders.push(s.to_ascii_lowercase());
            } else if !s.is_empty() {
                versions.push(s);
            }
        }
        let mut url = js(file.get("downloadUrl").unwrap_or(&Value::Null));
        if url.is_empty() {
            if let Ok(dl) = get_json(
                &format!(
                    "{CF}/mods/{}/files/{}/download-url",
                    hit.source_id,
                    ju(file.get("id").unwrap_or(&Value::Null))
                ),
                &cf_headers(cf_key),
            ) {
                url = js(dl.get("data").unwrap_or(&Value::Null));
            }
        }
        let deps = file
            .get("dependencies")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .map(|d| Dep {
                        project_id: ju(d.get("modId").unwrap_or(&Value::Null)).to_string(),
                        kind: cf_dep_kind(d.get("relationType").and_then(|v| v.as_i64()).unwrap_or(0))
                            .into(),
                        name: String::new(),
                        slug: String::new(),
                    })
                    .filter(|d| d.project_id != "0")
                    .collect()
            })
            .unwrap_or_default();
        return Ok(Some(ModFile {
            source: hit.source.clone(),
            source_id: hit.source_id.clone(),
            name: hit.name.clone(),
            slug: hit.slug.clone(),
            file_name: js(file
                .get("fileName")
                .or_else(|| file.get("displayName"))
                .unwrap_or(&Value::Null)),
            url,
            loaders,
            game_versions: versions,
            dependencies: deps,
        }));
    }
    Ok(None)
}

pub fn fill_dep_names(files: &mut [ModFile], cf_key: &str) {
    let mut mr_ids = Vec::new();
    let mut cf_ids = Vec::new();
    for f in files.iter() {
        for d in &f.dependencies {
            if f.source == "modrinth" {
                mr_ids.push(d.project_id.clone());
            } else {
                cf_ids.push(d.project_id.clone());
            }
        }
    }
    mr_ids.sort();
    mr_ids.dedup();
    cf_ids.sort();
    cf_ids.dedup();
    let mut names: std::collections::HashMap<String, (String, String)> =
        std::collections::HashMap::new();
    if !mr_ids.is_empty() {
        if let Ok(ids) = serde_json::to_string(&mr_ids) {
            let req = agent().get(&format!("{MR}/projects")).query("ids", &ids);
            if let Ok(payload) = req.call().map_err(|e| anyhow!("{}", describe_err(e))).and_then(|r| {
                r.into_json::<Value>().map_err(|e| anyhow!("{e}"))
            }) {
                if let Some(arr) = payload.as_array() {
                    for p in arr {
                        let id = js(p.get("id").unwrap_or(&Value::Null));
                        names.insert(
                            format!("modrinth:{id}"),
                            (
                                js(p.get("title").unwrap_or(&Value::Null)),
                                js(p.get("slug").unwrap_or(&Value::Null)),
                            ),
                        );
                    }
                }
            }
        }
    }
    if !cf_ids.is_empty() && !cf_key.is_empty() {
        let ids: Vec<i64> = cf_ids.iter().filter_map(|s| s.parse().ok()).collect();
        if !ids.is_empty() {
            let body = json!({ "modIds": ids, "filterPcOnly": true });
            let req = agent()
                .post(&format!("{CF}/mods"))
                .set("Accept", "application/json")
                .set("x-api-key", cf_key)
                .send_json(body);
            if let Ok(resp) = req {
                if let Ok(payload) = resp.into_json::<Value>() {
                    if let Some(arr) = payload.get("data").and_then(|v| v.as_array()) {
                        for p in arr {
                            let id = ju(p.get("id").unwrap_or(&Value::Null)).to_string();
                            names.insert(
                                format!("curseforge:{id}"),
                                (
                                    js(p.get("name").unwrap_or(&Value::Null)),
                                    js(p.get("slug").unwrap_or(&Value::Null)),
                                ),
                            );
                        }
                    }
                }
            }
        }
    }
    for f in files.iter_mut() {
        for d in &mut f.dependencies {
            if let Some((n, s)) = names.get(&format!("{}:{}", f.source, d.project_id)) {
                d.name = n.clone();
                d.slug = s.clone();
            } else if d.name.is_empty() {
                d.name = d.project_id.clone();
            }
        }
    }
}

pub fn fetch_mod_page(hit: &ModHit, cf_key: &str) -> Result<ModPage> {
    if hit.source == "modrinth" {
        let v = get_json(&format!("{MR}/project/{}", hit.source_id), &[])?;
        let mut body = js(v.get("body").unwrap_or(&Value::Null));
        if body.is_empty() {
            body = js(v.get("description").unwrap_or(&Value::Null));
        }
        if body.is_empty() {
            body = hit.summary.clone();
        }
        return Ok(ModPage {
            body,
            source_url: js(v.get("source_url").unwrap_or(&Value::Null)),
            issues_url: js(v.get("issues_url").unwrap_or(&Value::Null)),
            wiki_url: js(v.get("wiki_url").unwrap_or(&Value::Null)),
            discord_url: js(v.get("discord_url").unwrap_or(&Value::Null)),
        });
    }
    if hit.source == "curseforge" {
        if cf_key.trim().is_empty() {
            return Err(anyhow!("CurseForge API key required"));
        }
        let headers = cf_headers(cf_key);
        let v = get_json(&format!("{CF}/mods/{}", hit.source_id), &headers)?;
        let data = v.get("data").cloned().unwrap_or(v);
        let links = data.get("links").cloned().unwrap_or(Value::Null);
        let mut body = hit.summary.clone();
        if let Ok(desc) = get_json(
            &format!("{CF}/mods/{}/description", hit.source_id),
            &headers,
        ) {
            let html = js(desc.get("data").unwrap_or(&Value::Null));
            if !html.is_empty() {
                body = html;
            }
        }
        return Ok(ModPage {
            body,
            source_url: js(links.get("sourceUrl").unwrap_or(&Value::Null)),
            issues_url: js(links.get("issuesUrl").unwrap_or(&Value::Null)),
            wiki_url: js(links.get("wikiUrl").unwrap_or(&Value::Null)),
            discord_url: String::new(),
        });
    }
    Ok(ModPage {
        body: hit.summary.clone(),
        ..Default::default()
    })
}


