//! Named regex counters: match counts over complete log lines.

use std::fs;
use std::path::{Path, PathBuf};

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CounterDef {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub pattern: String,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl Default for CounterDef {
    fn default() -> Self {
        Self::new()
    }
}

impl CounterDef {
    pub fn new() -> Self {
        Self {
            name: String::new(),
            pattern: String::new(),
            case_sensitive: false,
            enabled: true,
        }
    }

    /// Label for the grid and status summary.
    pub fn display_name(&self) -> &str {
        let name = self.name.trim();
        if !name.is_empty() {
            return name;
        }
        let pat = self.pattern.trim();
        if !pat.is_empty() {
            return pat;
        }
        "(unnamed)"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterFile {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub counters: Vec<CounterDef>,
}

fn default_version() -> u32 {
    1
}

/// `%USERPROFILE%\Documents\FoxTail`, or `FoxTail` if the profile is missing.
pub fn default_dir() -> PathBuf {
    match std::env::var_os("USERPROFILE") {
        Some(profile) => PathBuf::from(profile).join("Documents").join("FoxTail"),
        None => PathBuf::from("FoxTail"),
    }
}

pub fn save_to(path: &Path, counters: &[CounterDef]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let file = CounterFile {
        version: 1,
        counters: counters.to_vec(),
    };
    let text = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    fs::write(path, text).map_err(|e| e.to_string())
}

pub fn load_from(path: &Path) -> Result<Vec<CounterDef>, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let file: CounterFile = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(file.counters)
}

enum Slot {
    Idle,
    Invalid,
    Regex(Regex),
}

/// Compiled counters. Rebuild when definitions change.
pub struct CompiledCounters {
    entries: Vec<Slot>,
}

impl CompiledCounters {
    pub fn new(defs: &[CounterDef]) -> Self {
        let entries = defs
            .iter()
            .map(|d| {
                if !d.enabled || d.pattern.is_empty() {
                    return Slot::Idle;
                }
                match RegexBuilder::new(&d.pattern)
                    .case_insensitive(!d.case_sensitive)
                    .build()
                {
                    Ok(re) => Slot::Regex(re),
                    Err(_) => Slot::Invalid,
                }
            })
            .collect();
        Self { entries }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_invalid(&self, index: usize) -> bool {
        matches!(self.entries.get(index), Some(Slot::Invalid))
    }

    pub fn has_work(&self) -> bool {
        self.entries.iter().any(|s| matches!(s, Slot::Regex(_)))
    }

    /// Add this line's non-overlapping matches into `matches` / `lines`.
    pub fn accumulate(&self, line: &str, matches: &mut [u64], lines: &mut [u64]) {
        let n = self.entries.len().min(matches.len()).min(lines.len());
        for i in 0..n {
            if let Slot::Regex(re) = &self.entries[i] {
                let hits = re.find_iter(line).count() as u64;
                if hits > 0 {
                    matches[i] += hits;
                    lines[i] += 1;
                }
            }
        }
    }

    /// Match count on a single line (tests and one-off use).
    pub fn count_line(&self, line: &str) -> Vec<u64> {
        self.entries
            .iter()
            .map(|s| match s {
                Slot::Regex(re) => re.find_iter(line).count() as u64,
                _ => 0,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(name: &str, pattern: &str, case_sensitive: bool) -> CounterDef {
        CounterDef {
            name: name.into(),
            pattern: pattern.into(),
            case_sensitive,
            enabled: true,
        }
    }

    #[test]
    fn two_matches_on_one_line() {
        let c = CompiledCounters::new(&[def("E", "ERROR", false)]);
        assert_eq!(c.count_line("ERROR then ERROR again"), vec![2]);
    }

    #[test]
    fn case_insensitive_by_default() {
        let c = CompiledCounters::new(&[def("", "error", false)]);
        assert_eq!(c.count_line("this is an Error"), vec![1]);
    }

    #[test]
    fn case_sensitive_toggle() {
        let c = CompiledCounters::new(&[def("", "error", true)]);
        assert_eq!(c.count_line("this is an Error"), vec![0]);
        assert_eq!(c.count_line("this is an error"), vec![1]);
    }

    #[test]
    fn disabled_and_empty_and_invalid_are_zero() {
        let mut off = def("x", "ERROR", false);
        off.enabled = false;
        let empty = def("y", "", false);
        let bad = def("z", "(", false);
        let c = CompiledCounters::new(&[off, empty, bad]);
        assert!(c.is_invalid(2));
        assert!(!c.has_work());
        assert_eq!(c.count_line("ERROR ("), vec![0, 0, 0]);
    }

    #[test]
    fn display_name_falls_back_to_pattern() {
        assert_eq!(def("", r"\bERROR\b", false).display_name(), r"\bERROR\b");
        assert_eq!(def("  ", "", false).display_name(), "(unnamed)");
        assert_eq!(def(" Errors ", "x", false).display_name(), "Errors");
    }

    #[test]
    fn accumulate_counts_matches_and_lines() {
        let c = CompiledCounters::new(&[def("E", "ERROR", false)]);
        let mut matches = [0u64];
        let mut lines = [0u64];
        c.accumulate("ERROR ERROR", &mut matches, &mut lines);
        c.accumulate("ok", &mut matches, &mut lines);
        c.accumulate("one ERROR", &mut matches, &mut lines);
        assert_eq!(matches, [3]);
        assert_eq!(lines, [2]);
    }

    #[test]
    fn json_round_trip() {
        let path = std::env::temp_dir().join(format!(
            "foxtail-counters-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let defs = vec![def("Errors", r"\bERROR\b", false)];
        save_to(&path, &defs).expect("save");
        let loaded = load_from(&path).expect("load");
        let _ = fs::remove_file(&path);
        assert_eq!(loaded, defs);
    }

    #[test]
    fn load_ignores_unknown_fields() {
        let path = std::env::temp_dir().join(format!(
            "foxtail-counters-extra-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::write(
            &path,
            r#"{"version":1,"extra":true,"counters":[{"name":"A","pattern":"x","future":1}]}"#,
        )
        .unwrap();
        let loaded = load_from(&path).expect("load");
        let _ = fs::remove_file(&path);
        assert_eq!(loaded[0].name, "A");
        assert_eq!(loaded[0].pattern, "x");
        assert!(loaded[0].enabled);
    }
}
