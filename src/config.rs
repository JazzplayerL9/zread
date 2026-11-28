use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub library_dir: String,
    pub extra_dirs: Vec<String>,
    pub theme: String,
    pub margin: u16,
    pub compact_paragraphs: bool,
    pub zen_mode: bool,
    pub wpm: u16,
    pub resume_last_book: bool,
    pub esc_quit_hold_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            library_dir: "~/Books".to_string(),
            extra_dirs: vec!["~/Documents".to_string()],
            theme: "Warm Dark".to_string(),
            margin: 1,
            compact_paragraphs: false,
            zen_mode: false,
            wpm: 260,
            resume_last_book: true,
            esc_quit_hold_ms: 3000,
        }
    }
}

impl Config {
    pub fn config_path() -> PathBuf {
        let home = home_dir();
        home.join(".config/zread/config.json")
    }

    pub fn load_or_create() -> Self {
        let path = Self::config_path();
        Self::load_from(&path)
    }

    pub fn load_from(path: &PathBuf) -> Self {
        if let Ok(raw) = std::fs::read_to_string(path) {
            if let Ok(cfg) = serde_json::from_str::<Config>(&raw) {
                return cfg;
            }
        }
        let cfg = Config::default();
        cfg.save_to(path);
        cfg
    }

    pub fn save(&self) {
        self.save_to(&Self::config_path());
    }

    pub fn save_to(&self, path: &PathBuf) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn resolved_library_dir(&self) -> PathBuf {
        expand_tilde(&self.library_dir)
    }

    pub fn resolved_scan_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = vec![self.resolved_library_dir()];
        for d in &self.extra_dirs {
            let p = expand_tilde(d);
            if !dirs.contains(&p) {
                dirs.push(p);
            }
        }
        dirs
    }
}

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from)
}

pub fn expand_tilde(raw: &str) -> PathBuf {
    if raw == "~" {
        return home_dir();
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        return home_dir().join(rest);
    }
    PathBuf::from(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tilde_expansion_and_scan_dirs() {
        let cfg = Config::default();
        let dirs = cfg.resolved_scan_dirs();
        assert_eq!(dirs.len(), 2);
        assert_eq!(cfg.resolved_library_dir(), home_dir().join("Books"));
        assert!(Config::config_path().ends_with("zread/config.json"));
        assert!(expand_tilde("~/Books").ends_with("Books"));
        assert_eq!(expand_tilde("~"), home_dir());
        assert_eq!(expand_tilde("/tmp/books"), PathBuf::from("/tmp/books"));
    }

    #[test]
    fn config_save_and_load_roundtrip() {
        let path = std::env::temp_dir().join(format!(
            "zread_config_test_{}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let cfg = Config {
            margin: 2,
            compact_paragraphs: true,
            zen_mode: true,
            ..Config::default()
        };
        cfg.save_to(&path);
        let loaded = Config::load_from(&path);
        assert_eq!(loaded.margin, 2);
        assert!(loaded.compact_paragraphs);
        assert!(loaded.zen_mode);
        let _ = std::fs::remove_file(&path);
    }
}
