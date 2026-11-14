use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;

/// Optional `~/.config/cozo-ng/config.toml`:
///
/// ```toml
/// default_db = "dev"
/// token = "…"                 # default token for remote targets
/// [aliases]
/// dev  = "sqlite:./dev.db"
/// prod = "https://cozo.example.com"
/// ```
#[derive(Debug, Default, Deserialize)]
pub struct Config {
    pub default_db: Option<String>,
    pub token: Option<String>,
    #[serde(default)]
    pub aliases: BTreeMap<String, String>,
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("cozo-ng").join("config.toml"))
    }

    pub fn load() -> Config {
        let Some(path) = Self::path() else { return Config::default() };
        let Ok(text) = std::fs::read_to_string(&path) else { return Config::default() };
        match toml::from_str(&text) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("warning: ignoring {}: {e}", path.display());
                Config::default()
            }
        }
    }

    pub fn resolve(&self, db: &str) -> String {
        self.aliases.get(db).cloned().unwrap_or_else(|| db.to_string())
    }
}

pub fn history_path() -> Option<PathBuf> {
    let dir = dirs::data_dir()?.join("cozo-ng");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("history"))
}
