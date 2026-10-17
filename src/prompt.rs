use std::collections::{HashMap, HashSet};

use crate::api::{self, key, ModHit};

#[derive(Clone, Debug)]
pub struct PromptPlan {
    pub raw: String,
    pub version: String,
    pub loader: String,
    pub project_type: String,
    pub queries: Vec<String>,
    pub themes: Vec<String>,
    pub tokens: Vec<String>,
}

const STOP: &[&str] = &[
    "a", "an", "the", "for", "with", "and", "or", "to", "of", "in", "on", "my", "me", "i", "we",
    "want", "need", "looking", "some", "any", "mods", "mod", "pack", "please", "that", "this",
    "like", "also", "plus", "using", "from", "into", "able", "make", "get", "add", "have", "has",
    "based", "upon", "about", "something", "stuff", "things", "good", "best", "play", "playing",
    "minecraft", "mc", "game", "server", "client", "version", "loader", "support", "supports",
    "compatible", "works", "working", "just", "really", "very", "more", "lot", "lots",
    "adding", "include", "including", "included", "related", "ones", "such",
];

const THEMES: &[(&[&str], &[&str], &str)] = &[
    (
        &[
            "performance",
            "fps",
            "optimize",
            "optimization",
            "optimise",
            "lag",
            "optifine",
            "culling",
            "smooth",
            "smoother",
            "lightweight",
        ],
        &[
            "sodium",
            "lithium",
            "starlight",
            "ferritecore",
            "entity culling",
            "modernfix",
            "immediatelyfast",
            "more culling",
            "krypton",
        ],
        "optimization",
    ),
    (
        &[
            "light",
            "lights",
            "lighting",
            "lightning",
            "rendering",
            "dynamic lights",
            "flashlight",
            "emissive",
            "glow",
            "bloom",
            "phosphor",
            "indium",
        ],
        &[
            "colorful lighting",
            "colourful lighting",
            "lambdynamiclights",
            "dynamic lights",
            "starlight",
            "sodium extra",
            "indium",
            "iris",
            "continuity",
        ],
        "lighting",
    ),
    (
        &["shader", "shaders", "shaderpack"],
        &["iris", "oculus", "complementary shaders"],
        "shaders",
    ),
    (
        &["minimap", "mini-map"],
        &["xaero's minimap", "journeymap"],
        "minimap",
    ),
    (
        &["worldmap", "world-map", "map"],
        &["xaero's world map", "journeymap"],
        "map",
    ),
    (
        &["jei", "rei", "emi", "recipes", "recipe"],
        &["jei", "roughly enough items", "emi"],
        "recipes",
    ),
    (
        &["inventory", "inv"],
        &["mouse tweaks", "inventory profiles", "jei"],
        "inventory",
    ),
    (
        &["qol", "quality"],
        &["jade", "appleskin", "mod menu", "controlling"],
        "qol",
    ),
    (
        &["create"],
        &["create", "create steam n rails", "create additions"],
        "create",
    ),
    (
        &[
            "mekanism",
            "tech",
            "technology",
            "machines",
            "automation",
            "industrial",
            "factory",
            "energy",
            "nuclear",
            "power",
        ],
        &[
            "mekanism",
            "mekanism generators",
            "immersive engineering",
            "thermal expansion",
            "industrial foregoing",
            "applied energistics",
            "create",
            "powah",
        ],
        "tech",
    ),
    (
        &[
            "magic",
            "wizard",
            "spells",
            "mana",
            "sorcery",
            "witch",
            "occult",
            "enchant",
            "ars",
            "botania",
        ],
        &[
            "ars nouveau",
            "botania",
            "iron's spells and spellbooks",
            "forbidden and arcanus",
            "occultism",
            "blood magic",
            "apotheosis",
        ],
        "magic",
    ),
    (
        &["storage", "chests", "pipes"],
        &["tom's simple storage", "refined storage", "applied energistics"],
        "storage",
    ),
    (
        &["worldgen", "terrain", "biomes", "landscape"],
        &["terralith", "tectonic", "biomes o plenty"],
        "worldgen",
    ),
    (
        &["structures", "dungeons", "exploration"],
        &["yung's", "when dungeons arise", "better desert temples"],
        "structures",
    ),
    (
        &["farming", "farm", "food", "crops"],
        &["farmer's delight"],
        "farming",
    ),
    (
        &["furniture", "decoration", "building", "decor"],
        &["another furniture", "macaw", "chipped"],
        "decoration",
    ),
    (
        &["combat", "pvp", "swords", "rpg"],
        &["better combat", "simply swords"],
        "combat",
    ),
    (
        &["mobs", "animals", "creatures"],
        &["alex's mobs", "alex's delight"],
        "mobs",
    ),
    (
        &["voice", "proximity", "chat"],
        &["simple voice chat"],
        "voice",
    ),
    (
        &["distant", "lod", "horizon"],
        &["distant horizons"],
        "render",
    ),
    (
        &["caves", "cave"],
        &["yung's caves", "deeper and darker"],
        "caves",
    ),
    (
        &["multiplayer", "friends"],
        &["simple voice chat", "essential"],
        "multiplayer",
    ),
];

pub fn parse(text: &str) -> PromptPlan {
    let lower = text.to_ascii_lowercase();
    let mut version = String::new();
    if let Some(v) = extract_version(&lower) {
        version = v;
    }
    let mut loader = String::new();
    for (word, id) in [
        ("neoforge", "neoforge"),
        ("neo forge", "neoforge"),
        ("fabric", "fabric"),
        ("quilt", "quilt"),
        ("forge", "forge"),
    ] {
        if lower.contains(word) {
            loader = id.into();
            break;
        }
    }
    let mut project_type = String::new();
    if lower.contains("shaderpack") || (lower.contains("shader") && lower.contains("pack")) {
        project_type = "shader".into();
    } else if lower.contains("resource pack") || lower.contains("texture pack") {
        project_type = "resourcepack".into();
    } else if lower.contains("modpack") {
        project_type = "modpack".into();
    }

    let raw = text.trim().to_string();
    let mut themes = Vec::new();
    let mut queries = Vec::new();
    if !raw.is_empty() {
        queries.push(raw.clone());
    }
    let mut consumed: HashSet<String> = HashSet::new();
    for (keys, expansions, theme) in THEMES {
        if keys.iter().any(|k| contains_word(&lower, k)) {
            themes.push((*theme).to_string());
            for q in *expansions {
                push_query(&mut queries, q);
            }
            for k in *keys {
                consumed.insert((*k).to_string());
            }
        }
    }

    for token in tokenize(&lower) {
        if STOP.contains(&token.as_str())
            || token == version
            || token == loader
            || consumed.iter().any(|c| c.split_whitespace().any(|w| w == token))
            || is_version_token(&token)
        {
            continue;
        }
        if token.len() >= 3 {
            push_query(&mut queries, &token);
        }
    }

    if queries.is_empty() {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            queries.push(trimmed.to_string());
        }
    }
    queries.truncate(14);

    let tokens = tokenize(&lower)
        .into_iter()
        .filter(|t| t.len() >= 3 && !STOP.contains(&t.as_str()) && !is_version_token(t))
        .collect();

    PromptPlan {
        raw,
        version,
        loader,
        project_type,
        queries,
        themes,
        tokens,
    }
}

pub fn is_description(plan: &PromptPlan, raw: &str) -> bool {
    plan.queries.len() > 1
        || !plan.themes.is_empty()
        || raw.split_whitespace().count() >= 5
        || !plan.version.is_empty()
        || !plan.loader.is_empty()
}

pub fn summary(plan: &PromptPlan) -> String {
    let mut parts = Vec::new();
    if !plan.loader.is_empty() {
        parts.push(plan.loader.clone());
    }
    if !plan.version.is_empty() {
        parts.push(plan.version.clone());
    }
    if !plan.themes.is_empty() {
        parts.push(plan.themes.join(", "));
    }
    let extra: Vec<String> = plan
        .tokens
        .iter()
        .filter(|t| *t != &plan.loader && *t != &plan.version && !plan.themes.iter().any(|th| th == *t))
        .cloned()
        .collect();
    if !extra.is_empty() {
        parts.push(extra.join(" "));
    } else if plan.themes.is_empty() && !plan.raw.is_empty() {
        parts.push(plan.raw.clone());
    }
    if parts.is_empty() && !plan.raw.is_empty() {
        parts.push(plan.raw.clone());
    }
    parts.join(" · ")
}

pub fn run(
    plan: &PromptPlan,
    source: &str,
    project_type: &str,
    loader: &str,
    version: &str,
    cf_key: &str,
) -> (Vec<ModHit>, u64, Vec<String>) {
    let mut warnings = Vec::new();
    let mut scored: HashMap<String, (ModHit, i64)> = HashMap::new();
    let ptype = if !project_type.is_empty() {
        project_type
    } else if !plan.project_type.is_empty() {
        &plan.project_type
    } else {
        "mod"
    };
    let loader = if !loader.is_empty() {
        loader
    } else {
        &plan.loader
    };
    let version = if !version.is_empty() {
        version
    } else {
        &plan.version
    };
    let local = crate::catalog::prompt_search(plan, source, loader, version, ptype);
    for m in &local {
        let add = score(m, plan) + 20;
        scored.insert(crate::api::key(m), (m.clone(), add));
    }
    if !local.is_empty() {
        warnings.push("Mixed with local catalog.".into());
    }
    let mut api_queries = Vec::new();
    if !plan.raw.is_empty() {
        api_queries.push(plan.raw.clone());
    }
    for q in &plan.queries {
        if api_queries.len() >= 6 {
            break;
        }
        if !api_queries.iter().any(|x| x.eq_ignore_ascii_case(q)) {
            api_queries.push(q.clone());
        }
    }
    for q in &api_queries {
        let (hits, _, w) = api::search(q, source, ptype, loader, version, "relevance", 0, 16, cf_key);
        warnings.extend(w);
        for (i, hit) in hits.into_iter().enumerate() {
            let bonus = 16 - i as i64;
            let add = score(&hit, plan) + bonus;
            let k = key(&hit);
            scored
                .entry(k)
                .and_modify(|(_, s)| *s += add)
                .or_insert((hit, add));
        }
    }
    warnings.sort();
    warnings.dedup();
    let mut list: Vec<(ModHit, i64)> = scored.into_values().collect();
    list.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.downloads.cmp(&a.0.downloads)));
    let total = list.len() as u64;
    let hits = list.into_iter().take(50).map(|(m, _)| m).collect();
    (hits, total, warnings)
}

pub fn rank_local(hits: Vec<ModHit>, plan: &PromptPlan) -> Vec<ModHit> {
    let mut list: Vec<(ModHit, i64)> = hits
        .into_iter()
        .map(|m| {
            let s = score(&m, plan);
            (m, s)
        })
        .filter(|(_, s)| *s > 0 || plan.queries.is_empty())
        .collect();
    list.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.downloads.cmp(&a.0.downloads)));
    list.into_iter().map(|(m, _)| m).collect()
}

fn score(m: &ModHit, plan: &PromptPlan) -> i64 {
    let name = m.name.to_ascii_lowercase();
    let slug = m.slug.to_ascii_lowercase();
    let blob = format!(
        "{} {} {} {}",
        name,
        slug,
        m.summary.to_ascii_lowercase(),
        m.categories.join(" ").to_ascii_lowercase()
    );
    let mut s = 0i64;
    let raw = plan.raw.to_ascii_lowercase();
    if !raw.is_empty() {
        if name == raw || slug == raw {
            s += 200;
        } else if name.contains(&raw) || slug.contains(&raw) {
            s += 140;
        }
    }
    let mut name_tokens = 0i64;
    for t in &plan.tokens {
        if name.contains(t) || slug.contains(t) {
            name_tokens += 1;
            s += 35;
        } else if blob.contains(t) {
            s += 6;
        }
    }
    if !plan.tokens.is_empty() && name_tokens == plan.tokens.len() as i64 {
        s += 80;
    }
    for q in &plan.queries {
        let ql = q.to_ascii_lowercase();
        if ql == raw {
            continue;
        }
        if name == ql {
            s += 80;
        } else if name.contains(&ql) || slug.contains(&ql) {
            s += 40;
        } else if blob.contains(&ql) {
            s += 12;
        }
    }
    for theme in &plan.themes {
        if blob.contains(theme) {
            s += 8;
        }
    }
    s
}

fn push_query(queries: &mut Vec<String>, q: &str) {
    let q = q.trim();
    if q.is_empty() {
        return;
    }
    if queries.iter().any(|x| x.eq_ignore_ascii_case(q)) {
        return;
    }
    queries.push(q.to_string());
}

fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric() && c != '.')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn contains_word(hay: &str, needle: &str) -> bool {
    if needle.contains(' ') {
        return hay.contains(needle);
    }
    hay.split(|c: char| !c.is_ascii_alphanumeric())
        .any(|w| w == needle)
}

fn is_version_token(t: &str) -> bool {
    extract_version(t).is_some() && t.starts_with('1')
}

fn extract_version(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 2 < bytes.len() {
        if bytes[i] == b'1' && bytes[i + 1] == b'.' && bytes[i + 2].is_ascii_digit() {
            let start = i;
            i += 2;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            let v = &text[start..i];
            if v.matches('.').count() >= 1 && v.matches('.').count() <= 2 {
                let ok_before = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
                let ok_after = i == bytes.len() || !bytes[i].is_ascii_alphanumeric();
                if ok_before && ok_after {
                    return Some(v.to_string());
                }
            }
            continue;
        }
        i += 1;
    }
    None
}
