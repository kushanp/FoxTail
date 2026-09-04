use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

use crate::highlight::contains_ignore_ascii_case;

/// Include / exclude filter, applied in that order.
///
/// Empty include = show everything (then exclude still applies).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LineFilter {
    #[serde(default)]
    pub include: String,
    #[serde(default)]
    pub exclude: String,
    #[serde(default)]
    pub is_regex: bool,
    #[serde(default)]
    pub case_sensitive: bool,
}

impl LineFilter {
    pub fn is_active(&self) -> bool {
        !self.include.is_empty() || !self.exclude.is_empty()
    }

    pub fn compile(&self) -> CompiledFilter {
        CompiledFilter {
            include: compile_pat(&self.include, self.is_regex, self.case_sensitive),
            exclude: compile_pat(&self.exclude, self.is_regex, self.case_sensitive),
            case_sensitive: self.case_sensitive,
            is_regex: self.is_regex,
            include_raw: self.include.clone(),
            exclude_raw: self.exclude.clone(),
        }
    }
}

#[derive(Debug, Clone)]
enum Pat {
    Empty,
    Substr(String),
    Regex(Regex),
    Invalid,
}

fn compile_pat(raw: &str, is_regex: bool, case_sensitive: bool) -> Pat {
    if raw.is_empty() {
        return Pat::Empty;
    }
    if is_regex {
        match RegexBuilder::new(raw)
            .case_insensitive(!case_sensitive)
            .build()
        {
            Ok(re) => Pat::Regex(re),
            Err(_) => Pat::Invalid,
        }
    } else {
        Pat::Substr(raw.to_string())
    }
}

impl Pat {
    fn matches(&self, line: &str, case_sensitive: bool) -> bool {
        match self {
            Pat::Empty => true,
            Pat::Invalid => false,
            Pat::Regex(re) => re.is_match(line),
            Pat::Substr(s) => {
                if case_sensitive {
                    line.contains(s)
                } else {
                    contains_ignore_ascii_case(line, s)
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompiledFilter {
    include: Pat,
    exclude: Pat,
    case_sensitive: bool,
    is_regex: bool,
    include_raw: String,
    exclude_raw: String,
}

impl CompiledFilter {
    pub fn allows(&self, line: &str) -> bool {
        if !self.include_raw.is_empty() && !self.include.matches(line, self.case_sensitive) {
            return false;
        }
        if !self.exclude_raw.is_empty() && self.exclude.matches(line, self.case_sensitive) {
            return false;
        }
        true
    }

    pub fn include_error(&self) -> bool {
        self.is_regex && !self.include_raw.is_empty() && matches!(self.include, Pat::Invalid)
    }

    pub fn exclude_error(&self) -> bool {
        self.is_regex && !self.exclude_raw.is_empty() && matches!(self.exclude, Pat::Invalid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn include_only() {
        let f = LineFilter {
            include: "ERROR".into(),
            ..Default::default()
        }
        .compile();
        assert!(f.allows("an ERROR happened"));
        assert!(!f.allows("all good"));
    }

    #[test]
    fn exclude_only() {
        let f = LineFilter {
            exclude: "DEBUG".into(),
            ..Default::default()
        }
        .compile();
        assert!(f.allows("INFO hello"));
        assert!(!f.allows("DEBUG hello"));
    }

    #[test]
    fn include_then_exclude() {
        let f = LineFilter {
            include: "http".into(),
            exclude: "200".into(),
            ..Default::default()
        }
        .compile();
        assert!(f.allows("http 404"));
        assert!(!f.allows("http 200"));
        assert!(!f.allows("disk 404"));
    }
}
