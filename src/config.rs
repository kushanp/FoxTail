use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::highlight::{default_rules, HighlightRule};

const RECENT_MAX: usize = 16;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    #[serde(default = "default_tab_width")]
    pub tab_width: usize,
    #[serde(default = "default_true")]
    pub follow_tail: bool,
    #[serde(default)]
    pub word_wrap: bool,
    #[serde(default = "default_true")]
    pub show_line_numbers: bool,
    #[serde(default)]
    pub always_on_top: bool,
    #[serde(default = "default_poll_ms")]
    pub poll_ms: u64,
    #[serde(default)]
    pub highlight_rules: Vec<HighlightRule>,
    #[serde(default)]
    pub recent_files: Vec<PathBuf>,
}

fn default_font_size() -> f32 {
    13.0
}
fn default_tab_width() -> usize {
    4
}
fn default_true() -> bool {
    true
}
fn default_poll_ms() -> u64 {
    75
}

impl Default for Config {
    fn default() -> Self {
        Self {
            font_size: default_font_size(),
            tab_width: default_tab_width(),
            follow_tail: true,
            word_wrap: false,
            show_line_numbers: true,
            always_on_top: false,
            poll_ms: default_poll_ms(),
            highlight_rules: default_rules(),
            recent_files: Vec::new(),
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let path = resolve_load_path();
        let Some(path) = path else {
            return Self::default();
        };
        match fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Config>(&text) {
                Ok(mut cfg) => {
                    if cfg.highlight_rules.is_empty() {
                        cfg.highlight_rules = default_rules();
                    }
                    if cfg.font_size < 8.0 {
                        cfg.font_size = 8.0;
                    }
                    if cfg.font_size > 48.0 {
                        cfg.font_size = 48.0;
                    }
                    if cfg.tab_width == 0 || cfg.tab_width > 16 {
                        cfg.tab_width = 4;
                    }
                    if cfg.poll_ms < 25 {
                        cfg.poll_ms = 25;
                    }
                    cfg
                }
                Err(_) => Self::default(),
            },
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) {
        let path = resolve_save_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, text);
        }
    }

    pub fn remember_file(&mut self, path: &Path) {
        self.recent_files.retain(|p| p != path);
        self.recent_files.insert(0, path.to_path_buf());
        self.recent_files.truncate(RECENT_MAX);
    }
}

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
}

fn appdata_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("FoxTail").join("config.json"))
}

/// Load order: cwd, next to the executable, then AppData.
fn resolve_load_path() -> Option<PathBuf> {
    let cwd = PathBuf::from("foxtail.json");
    if cwd.is_file() {
        return Some(cwd);
    }
    if let Some(dir) = exe_dir() {
        let p = dir.join("foxtail.json");
        if p.is_file() {
            return Some(p);
        }
    }
    let app = appdata_path()?;
    if app.is_file() {
        Some(app)
    } else {
        None
    }
}

fn resolve_save_path() -> PathBuf {
    let cwd = PathBuf::from("foxtail.json");
    if cwd.is_file() {
        return cwd;
    }
    if let Some(dir) = exe_dir() {
        let p = dir.join("foxtail.json");
        if p.is_file() {
            return p;
        }
    }
    appdata_path().unwrap_or_else(|| PathBuf::from("foxtail.json"))
}
