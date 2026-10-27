use std::collections::{HashMap, HashSet};

use crate::api::{key, ModHit};

pub struct Library {
    mods: HashMap<String, ModHit>,
    favorites: HashSet<String>,
}

impl Library {
    pub fn load() -> Self {
        let mut lib = Library {
            mods: HashMap::new(),
            favorites: HashSet::new(),
        };
        let dir = crate::util::data_dir();
        if let Ok(text) = std::fs::read_to_string(dir.join("library.json")) {
            if let Ok(list) = serde_json::from_str::<Vec<ModHit>>(&text) {
                for m in list {
                    lib.mods.insert(key(&m), m);
                }
            }
        }
        if let Ok(text) = std::fs::read_to_string(dir.join("favorites.txt")) {
            for line in text.lines() {
                let k = line.trim();
                if !k.is_empty() {
                    lib.favorites.insert(k.to_string());
                }
            }
        }
        lib
    }

    pub fn save(&self) {
        let dir = crate::util::data_dir();
        let list: Vec<&ModHit> = self.mods.values().collect();
        if let Ok(text) = serde_json::to_string_pretty(&list) {
            let _ = std::fs::write(dir.join("library.json"), text);
        }
        let fav = self.favorites.iter().cloned().collect::<Vec<_>>().join("\n");
        let _ = std::fs::write(dir.join("favorites.txt"), fav);
    }

    pub fn upsert(&mut self, hits: &[ModHit]) {
        for m in hits {
            self.mods.insert(key(m), m.clone());
        }
        self.save();
    }

    pub fn is_favorite(&self, m: &ModHit) -> bool {
        self.favorites.contains(&key(m))
    }

    pub fn toggle_favorite(&mut self, m: &ModHit) -> bool {
        let k = key(m);
        if !self.favorites.remove(&k) {
            self.favorites.insert(k);
            self.mods.insert(key(m), m.clone());
            self.save();
            return true;
        }
        self.save();
        false
    }

    pub fn search(
        &self,
        query: &str,
        source: &str,
        loader: &str,
        version: &str,
        project_type: &str,
        favorites_only: bool,
    ) -> Vec<ModHit> {
        let q = query.to_ascii_lowercase();
        let mut out: Vec<ModHit> = self
            .mods
            .values()
            .filter(|m| {
                if favorites_only && !self.favorites.contains(&key(m)) {
                    return false;
                }
                if source != "all" && source != "local" && m.source != source {
                    return false;
                }
                if !loader.is_empty() && !m.loaders.iter().any(|l| l.eq_ignore_ascii_case(loader)) {
                    return false;
                }
                if !version.is_empty() && !m.game_versions.iter().any(|v| v == version) {
                    return false;
                }
                if !project_type.is_empty()
                    && m.categories
                        .first()
                        .map(|c| c.as_str())
                        .unwrap_or("")
                        != project_type
                {
                    return false;
                }
                if q.is_empty() {
                    return true;
                }
                m.name.to_ascii_lowercase().contains(&q)
                    || m.author.to_ascii_lowercase().contains(&q)
                    || m.slug.to_ascii_lowercase().contains(&q)
                    || m.summary.to_ascii_lowercase().contains(&q)
            })
            .cloned()
            .collect();
        out.sort_by(|a, b| b.downloads.cmp(&a.downloads));
        out
    }

    pub fn stats(&self) -> (usize, usize, usize, usize) {
        let mr = self.mods.values().filter(|m| m.source == "modrinth").count();
        let cf = self.mods.values().filter(|m| m.source == "curseforge").count();
        (self.mods.len(), mr, cf, self.favorites.len())
    }
    pub fn list_all(&self) -> Vec<(ModHit, bool)> {
        let mut out: Vec<(ModHit, bool)> = self
            .mods
            .values()
            .map(|m| (m.clone(), self.favorites.contains(&key(m))))
            .collect();
        out.sort_by(|a, b| b.0.downloads.cmp(&a.0.downloads));
        out
    }

}
