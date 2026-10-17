use std::collections::{HashMap, HashSet};

use crate::api::{fill_dep_names, resolve_file, ModFile, ModHit};

#[derive(Clone, Debug)]
pub struct Issue {
    pub severity: String,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct PackReport {
    pub ok: bool,
    pub game_version: String,
    pub loader: String,
    pub issues: Vec<Issue>,
    pub files: Vec<ModFile>,
}

fn intersect(sets: &[HashSet<String>]) -> HashSet<String> {
    let mut iter = sets.iter();
    let Some(first) = iter.next() else {
        return HashSet::new();
    };
    let mut out = first.clone();
    for s in iter {
        out = out.intersection(s).cloned().collect();
    }
    out
}

pub fn check_pack(
    mods: &[ModHit],
    game_version: &str,
    loader: &str,
    cf_key: &str,
) -> PackReport {
    let mut issues = Vec::new();
    let mut game_version = game_version.to_string();
    let mut loader = loader.to_string();

    if mods.is_empty() {
        return PackReport {
            ok: true,
            game_version,
            loader,
            issues: vec![Issue {
                severity: "info".into(),
                message: "Select mods to check compatibility.".into(),
            }],
            files: Vec::new(),
        };
    }

    let version_sets: Vec<HashSet<String>> = mods
        .iter()
        .filter(|m| !m.game_versions.is_empty())
        .map(|m| m.game_versions.iter().cloned().collect())
        .collect();
    let loader_sets: Vec<HashSet<String>> = mods
        .iter()
        .filter(|m| !m.loaders.is_empty())
        .map(|m| m.loaders.iter().map(|s| s.to_ascii_lowercase()).collect())
        .collect();
    let common_v = intersect(&version_sets);
    let common_l = intersect(&loader_sets);

    if game_version.is_empty() {
        if common_v.len() == 1 {
            game_version = common_v.iter().next().cloned().unwrap_or_default();
        } else if common_v.is_empty() {
            issues.push(Issue {
                severity: "error".into(),
                message: "These mods share no Minecraft version.".into(),
            });
        } else {
            let mut sample: Vec<_> = common_v.iter().cloned().collect();
            sample.sort();
            sample.reverse();
            sample.truncate(8);
            issues.push(Issue {
                severity: "warn".into(),
                message: format!("Pick a Minecraft version. Shared: {}", sample.join(", ")),
            });
        }
    } else {
        let missing: Vec<_> = mods
            .iter()
            .filter(|m| !m.game_versions.is_empty() && !m.game_versions.iter().any(|v| v == &game_version))
            .map(|m| m.name.clone())
            .collect();
        if !missing.is_empty() {
            issues.push(Issue {
                severity: "error".into(),
                message: format!("No listed {game_version} support: {}", missing.join(", ")),
            });
        }
    }

    if loader.is_empty() {
        if common_l.len() == 1 {
            loader = common_l.iter().next().cloned().unwrap_or_default();
        } else if common_l.is_empty() {
            issues.push(Issue {
                severity: "error".into(),
                message: "These mods share no loader (Fabric / Forge / NeoForge / Quilt).".into(),
            });
        } else {
            let mut sample: Vec<_> = common_l.iter().cloned().collect();
            sample.sort();
            issues.push(Issue {
                severity: "warn".into(),
                message: format!("Pick a loader. Shared: {}", sample.join(", ")),
            });
        }
    } else {
        let missing: Vec<_> = mods
            .iter()
            .filter(|m| {
                !m.loaders.is_empty()
                    && !m.loaders.iter().any(|l| l.eq_ignore_ascii_case(&loader))
            })
            .map(|m| m.name.clone())
            .collect();
        if !missing.is_empty() {
            issues.push(Issue {
                severity: "error".into(),
                message: format!("Not listed for {loader}: {}", missing.join(", ")),
            });
        }
    }

    let mut by_slug: HashMap<String, Vec<String>> = HashMap::new();
    for m in mods {
        if !m.slug.is_empty() {
            by_slug
                .entry(m.slug.to_ascii_lowercase())
                .or_default()
                .push(format!("{} ({})", m.name, m.source));
        }
    }
    for labels in by_slug.values() {
        if labels.len() > 1 {
            issues.push(Issue {
                severity: "warn".into(),
                message: format!("Same mod selected twice: {}", labels.join(", ")),
            });
        }
    }

    let selected_ids: HashSet<String> = mods.iter().map(|m| format!("{}:{}", m.source, m.source_id)).collect();
    let selected_slugs: HashSet<String> = mods
        .iter()
        .map(|m| m.slug.to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    let mut files = Vec::new();
    for m in mods {
        match resolve_file(m, &game_version, &loader, cf_key) {
            Ok(Some(file)) => files.push(file),
            Ok(None) => {
                let target = format!("{} {}", loader, game_version).trim().to_string();
                issues.push(Issue {
                    severity: "error".into(),
                    message: format!("{} has no file for {target}", m.name),
                });
            }
            Err(e) => issues.push(Issue {
                severity: "error".into(),
                message: format!("{}: {e}", m.name),
            }),
        }
    }
    fill_dep_names(&mut files, cf_key);
    for (severity, message) in crate::github::pack_hints(mods, &game_version, &loader) {
        issues.push(Issue { severity, message });
    }
    for f in &files {
        for d in &f.dependencies {
            let present = selected_ids.contains(&format!("{}:{}", f.source, d.project_id))
                || (!d.slug.is_empty() && selected_slugs.contains(&d.slug.to_ascii_lowercase()));
            let label = if d.name.is_empty() {
                d.project_id.clone()
            } else {
                d.name.clone()
            };
            if d.kind == "incompatible" && present {
                issues.push(Issue {
                    severity: "error".into(),
                    message: format!("{} is marked incompatible with {label}", f.name),
                });
            } else if d.kind == "required" && !present {
                issues.push(Issue {
                    severity: "warn".into(),
                    message: format!("{} requires {label}, which is not in the selection", f.name),
                });
            }
        }
    }

    let mut seen = HashSet::new();
    issues.retain(|i| seen.insert(i.message.clone()));
    let ok = !issues.iter().any(|i| i.severity == "error");
    PackReport {
        ok,
        game_version,
        loader,
        issues,
        files,
    }
}
