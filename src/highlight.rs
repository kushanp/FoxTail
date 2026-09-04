use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

/// A single highlighting rule. First matching enabled rule wins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HighlightRule {
    pub pattern: String,
    #[serde(default)]
    pub is_regex: bool,
    #[serde(default)]
    pub case_sensitive: bool,
    pub fg: [u8; 3],
    pub bg: [u8; 3],
    #[serde(default)]
    pub bold: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl HighlightRule {
    pub fn new(pattern: impl Into<String>, fg: [u8; 3], bg: [u8; 3]) -> Self {
        Self {
            pattern: pattern.into(),
            is_regex: false,
            case_sensitive: false,
            fg,
            bg,
            bold: false,
            enabled: true,
        }
    }
}

/// Style applied to a matching line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineStyle {
    pub fg: [u8; 3],
    pub bg: [u8; 3],
    pub bold: bool,
}

/// Compiled highlighter. Rebuild when rules change.
#[derive(Debug, Default)]
pub struct Highlighter {
    entries: Vec<CompiledRule>,
}

struct CompiledRule {
    rule: HighlightRule,
    regex: Option<Regex>,
}

impl std::fmt::Debug for CompiledRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledRule")
            .field("pattern", &self.rule.pattern)
            .finish()
    }
}

impl Highlighter {
    pub fn new(rules: &[HighlightRule]) -> Self {
        let entries = rules
            .iter()
            .filter(|r| r.enabled && !r.pattern.is_empty())
            .map(|r| {
                let regex = if r.is_regex {
                    RegexBuilder::new(&r.pattern)
                        .case_insensitive(!r.case_sensitive)
                        .build()
                        .ok()
                } else {
                    None
                };
                CompiledRule {
                    rule: r.clone(),
                    regex,
                }
            })
            .collect();
        Self { entries }
    }

    /// First matching rule, or `None`.
    pub fn style_for(&self, line: &str) -> Option<LineStyle> {
        for entry in &self.entries {
            if entry.matches(line) {
                return Some(LineStyle {
                    fg: entry.rule.fg,
                    bg: entry.rule.bg,
                    bold: entry.rule.bold,
                });
            }
        }
        None
    }
}

impl CompiledRule {
    fn matches(&self, line: &str) -> bool {
        if self.rule.is_regex {
            return self
                .regex
                .as_ref()
                .is_some_and(|re| re.is_match(line));
        }
        if self.rule.case_sensitive {
            line.contains(&self.rule.pattern)
        } else {
            contains_ignore_ascii_case(line, &self.rule.pattern)
        }
    }
}

pub fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack
        .as_bytes()
        .windows(needle.len())
        .any(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
}

/// Built-in rules for typical log levels and failure words.
pub fn default_rules() -> Vec<HighlightRule> {
    vec![
        HighlightRule {
            pattern: r"\b(ERROR|FATAL|CRITICAL|SEVERE)\b".into(),
            is_regex: true,
            case_sensitive: false,
            fg: [255, 255, 255],
            bg: [160, 28, 28],
            bold: true,
            enabled: true,
        },
        HighlightRule {
            pattern: r"\b(WARN|WARNING)\b".into(),
            is_regex: true,
            case_sensitive: false,
            fg: [20, 16, 8],
            bg: [210, 160, 40],
            bold: true,
            enabled: true,
        },
        HighlightRule {
            pattern: r"\b(Exception|Failed|Failure|Timeout)\b".into(),
            is_regex: true,
            case_sensitive: false,
            fg: [255, 230, 240],
            bg: [110, 40, 90],
            bold: false,
            enabled: true,
        },
        HighlightRule {
            pattern: r"\bINFO\b".into(),
            is_regex: true,
            case_sensitive: false,
            fg: [140, 220, 255],
            bg: [24, 40, 56],
            bold: false,
            enabled: true,
        },
        HighlightRule {
            pattern: r"\b(DEBUG|TRACE)\b".into(),
            is_regex: true,
            case_sensitive: false,
            fg: [150, 150, 160],
            bg: [28, 28, 32],
            bold: false,
            enabled: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_rule_wins() {
        let rules = vec![
            HighlightRule::new("ERROR", [255, 255, 255], [255, 0, 0]),
            HighlightRule::new("ERR", [0, 0, 0], [255, 255, 0]),
        ];
        let h = Highlighter::new(&rules);
        let s = h.style_for("an ERROR occurred").unwrap();
        assert_eq!(s.bg, [255, 0, 0]);
    }

    #[test]
    fn case_insensitive_substring() {
        let rules = vec![HighlightRule::new("error", [1, 2, 3], [4, 5, 6])];
        let h = Highlighter::new(&rules);
        assert!(h.style_for("this is an Error").is_some());
        assert!(h.style_for("nope").is_none());
    }

    #[test]
    fn regex_rule() {
        let mut r = HighlightRule::new(r"\d{3}", [0, 0, 0], [1, 1, 1]);
        r.is_regex = true;
        let h = Highlighter::new(&[r]);
        assert!(h.style_for("status 404").is_some());
        assert!(h.style_for("status 4").is_none());
    }

    #[test]
    fn disabled_rules_are_skipped() {
        let mut r = HighlightRule::new("ERROR", [1, 1, 1], [2, 2, 2]);
        r.enabled = false;
        let h = Highlighter::new(&[r]);
        assert!(h.style_for("ERROR").is_none());
    }
}
