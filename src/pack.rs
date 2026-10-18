use crate::api::{self, ModHit};
use crate::compat;
use crate::prompt;

#[derive(Clone, Debug)]
pub struct PackPlan {
    pub game_version: String,
    pub loader: String,
    pub description: String,
    pub mods: Vec<ModHit>,
    pub report: Option<compat::PackReport>,
    pub note: String,
}

fn score(m: &ModHit, plan: &prompt::PromptPlan) -> i64 {
    let blob = format!(
        "{} {} {} {}",
        m.name.to_ascii_lowercase(),
        m.slug.to_ascii_lowercase(),
        m.summary.to_ascii_lowercase(),
        m.categories.join(" ").to_ascii_lowercase()
    );
    let raw = plan.raw.to_ascii_lowercase();
    let mut s = 0i64;
    if !raw.is_empty() {
        if m.name.to_ascii_lowercase().contains(&raw) || m.slug.to_ascii_lowercase().contains(&raw) {
            s += 120;
        }
    }
    let mut name_hits = 0i64;
    for t in &plan.tokens {
        if m.name.to_ascii_lowercase().contains(t) || m.slug.to_ascii_lowercase().contains(t) {
            name_hits += 1;
            s += 50;
        } else if blob.contains(t) {
            s += 10;
        }
    }
    if !plan.tokens.is_empty() && name_hits == plan.tokens.len() as i64 {
        s += 120;
    }
    for theme in &plan.themes {
        if blob.contains(theme) {
            s += 18;
        }
    }
    for q in &plan.queries {
        let ql = q.to_ascii_lowercase();
        if ql == raw {
            continue;
        }
        if m.name.to_ascii_lowercase() == ql {
            s += 80;
        } else if m.name.to_ascii_lowercase().contains(&ql) {
            s += 40;
        } else if blob.contains(&ql) {
            s += 12;
        }
    }
    s += (m.downloads.min(50_000_000) / 500_000) as i64;
    s
}

pub fn build(
    description: &str,
    game_version: &str,
    loader: &str,
    project_type: &str,
    cf_key: &str,
) -> PackPlan {
    let plan = prompt::parse(description);
    let mut seen = std::collections::HashSet::new();
    let mut cands: Vec<(ModHit, i64)> = Vec::new();
    let mut queries = Vec::new();
    if !plan.raw.is_empty() {
        queries.push(plan.raw.clone());
    }
    for q in &plan.queries {
        if queries.len() >= 8 {
            break;
        }
        if !queries.iter().any(|x| x.eq_ignore_ascii_case(q)) {
            queries.push(q.clone());
        }
    }
    for theme in &plan.themes {
        if queries.len() >= 10 {
            break;
        }
        if !queries.iter().any(|x| x.eq_ignore_ascii_case(theme)) {
            queries.push(theme.clone());
        }
    }
    for q in &queries {
        for source in ["modrinth", "curseforge"] {
            if source == "curseforge" && cf_key.trim().is_empty() {
                continue;
            }
            let (hits, _, _) = api::search(
                q,
                source,
                project_type,
                loader,
                game_version,
                "relevance",
                0,
                24,
                cf_key,
            );
for h in hits {
                    let k = api::key(&h);
                    if seen.insert(k) {
                        cands.push((h.clone(), score(&h, &plan)));
                    }
                }
        }
    }
    cands.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.downloads.cmp(&a.0.downloads)));
    let picked: Vec<ModHit> = cands.into_iter().take(14).map(|(m, _)| m).collect();
    let report = if picked.is_empty() {
        None
    } else {
        Some(compat::check_pack(&picked, game_version, loader, cf_key))
    };
    let note = if picked.is_empty() {
        "No mods matched that description. Try fewer words or a different loader/version.".into()
    } else if report.as_ref().map(|r| r.ok).unwrap_or(true) {
        format!("Picked {} mods — all compatible with {} {}", picked.len(), loader, game_version)
    } else {
        "Picked mods — some may need a different loader or version.".into()
    };
    PackPlan {
        game_version: game_version.to_string(),
        loader: loader.to_string(),
        description: description.to_string(),
        mods: picked,
        report,
        note,
    }
}