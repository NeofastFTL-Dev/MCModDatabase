use std::path::PathBuf;

const DEFAULT_CURSEFORGE_KEY: &str =
    "$2a$10$7.N6/h97BqTSTNjVynuU3O6eWkr5yzbVpZ1rtwrTqYmt98Rr/78zm";

#[derive(Clone, Debug)]
pub struct Settings {
    pub download_dir: PathBuf,
    pub curseforge_key: String,
    pub skip_update: String,
    pub agent_api_key: String,
    pub agent_port: u16,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            download_dir: crate::util::default_download_dir(),
            curseforge_key: DEFAULT_CURSEFORGE_KEY.to_string(),
            skip_update: String::new(),
            agent_api_key: String::new(),
            agent_port: 8765,
        }
    }
}

fn path() -> PathBuf {
    crate::util::data_dir().join("settings.ini")
}

impl Settings {
    pub fn load() -> Settings {
        let mut s = Settings::default();
        if let Ok(text) = std::fs::read_to_string(path()) {
            for line in text.lines() {
                if let Some((k, v)) = line.split_once('=') {
                    match k.trim() {
                        "download_dir" if !v.trim().is_empty() => {
                            s.download_dir = PathBuf::from(v.trim())
                        }
                        "curseforge_key" if !v.trim().is_empty() => {
                            s.curseforge_key = v.trim().to_string()
                        }
                        "skip_update" => s.skip_update = v.trim().to_string(),
                        "agent_api_key" => s.agent_api_key = v.trim().to_string(),
                        "agent_port" => {
                            if let Ok(p) = v.trim().parse::<u16>() {
                                if p > 0 {
                                    s.agent_port = p;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        s
    }

    pub fn save(&self) {
        let text = format!(
            "download_dir={}\ncurseforge_key={}\nskip_update={}\nagent_api_key={}\nagent_port={}\n",
            self.download_dir.display(),
            self.curseforge_key,
            self.skip_update,
            self.agent_api_key,
            self.agent_port
        );
        let _ = std::fs::write(path(), text);
    }
}
