use anyhow::{anyhow, Result};
use serde_json::Value;

use crate::net::{download_file, get_json};

const REPO: &str = "https://api.github.com/repos/NeofastFTL-Dev/MCModDatabase/releases/latest";
const HEADERS: &[(&str, &str)] = &[("Accept", "application/vnd.github+json")];

#[derive(Clone, Debug)]
pub struct Release {
    pub tag: String,
    pub name: String,
    pub notes: String,
    pub html_url: String,
    pub download_url: String,
    pub asset_name: String,
}

pub fn current() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn tag_digits(tag: &str) -> Vec<u32> {
    tag.trim()
        .trim_start_matches('v')
        .trim_start_matches('V')
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect()
}

pub fn is_newer(tag: &str, current: &str) -> bool {
    let a = tag_digits(tag);
    let b = tag_digits(current);
    let n = a.len().max(b.len());
    for i in 0..n {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        if x != y {
            return x > y;
        }
    }
    false
}

pub fn latest() -> Result<Option<Release>> {
    let v = get_json(REPO, HEADERS)?;
    let tag = js(&v["tag_name"]);
    if tag.is_empty() {
        return Err(anyhow!("no release tag"));
    }
    if !is_newer(&tag, current()) {
        return Ok(None);
    }
    let assets = v
        .get("assets")
        .and_then(|a| a.as_array())
        .cloned()
        .unwrap_or_default();
    let mut pick: Option<(String, String)> = None;
    for a in &assets {
        let name = js(&a["name"]);
        let url = js(&a["browser_download_url"]);
        if name.to_ascii_lowercase().ends_with(".exe") && !url.is_empty() {
            let score = name.to_ascii_lowercase().contains("opensource")
                || name.to_ascii_lowercase().contains("setup")
                || name.to_ascii_lowercase().contains("install");
            if score || pick.is_none() {
                pick = Some((name, url));
                if score {
                    break;
                }
            }
        }
    }
    let Some((asset_name, download_url)) = pick else {
        return Err(anyhow!("no installer on the latest release"));
    };
    let notes = js(&v["body"]);
    let notes = if notes.len() > 600 {
        let mut end = 600;
        while end > 0 && !notes.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &notes[..end])
    } else {
        notes
    };
    Ok(Some(Release {
        tag,
        name: js(&v["name"]),
        notes,
        html_url: js(&v["html_url"]),
        download_url,
        asset_name,
    }))
}

pub fn download_installer(rel: &Release) -> Result<std::path::PathBuf> {
    let dest = std::env::temp_dir().join(if rel.asset_name.is_empty() {
        "MCDB-update.exe".into()
    } else {
        crate::util::sanitize_component(&rel.asset_name)
    });
    download_file(&rel.download_url, &[], &dest, |_, _| {})?;
    Ok(dest)
}

pub fn launch_installer(path: &std::path::Path) -> Result<()> {
    std::process::Command::new(path).spawn()?;
    Ok(())
}

fn js(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}
