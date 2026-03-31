/**
 * @agents
 * Configuration loading from .git-agent-tags.toml or defaults.
 * Related: git-agent-tags/src/main.rs, git-agent-tags/src/check.rs
 */

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_ignore")]
    pub ignore: Vec<String>,
    #[serde(default = "default_stale_commit_gap")]
    pub stale_commit_gap: usize,
    #[serde(default = "default_stale_diff_percent")]
    pub stale_diff_percent: f64,
    #[serde(default = "default_cochange_min_commits")]
    pub cochange_min_commits: usize,
    #[serde(default = "default_cochange_max_files")]
    pub cochange_max_files: usize,
}

fn default_ignore() -> Vec<String> {
    vec![
        // Directories
        "node_modules".into(),
        "dist".into(),
        "build".into(),
        "target".into(),
        ".git".into(),
        // Documentation / plain text
        "*.md".into(),
        "*.mdx".into(),
        "*.rst".into(),
        "*.txt".into(),
        // Lock / checksum files
        "*.lock".into(),
        "*.sum".into(),
        // Images
        "*.png".into(),
        "*.jpg".into(),
        "*.jpeg".into(),
        "*.gif".into(),
        "*.svg".into(),
        "*.ico".into(),
        "*.webp".into(),
        // Fonts
        "*.woff".into(),
        "*.woff2".into(),
        "*.ttf".into(),
        "*.eot".into(),
        // Minified / compiled artefacts
        "*.min.js".into(),
        "*.min.css".into(),
        "*.map".into(),
    ]
}

fn default_stale_commit_gap() -> usize {
    10
}

fn default_stale_diff_percent() -> f64 {
    40.0
}

fn default_cochange_min_commits() -> usize {
    3
}

fn default_cochange_max_files() -> usize {
    20
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ignore: default_ignore(),
            stale_commit_gap: default_stale_commit_gap(),
            stale_diff_percent: default_stale_diff_percent(),
            cochange_min_commits: default_cochange_min_commits(),
            cochange_max_files: default_cochange_max_files(),
        }
    }
}

pub fn load_config(repo_root: &Path) -> Config {
    let config_path = repo_root.join(".git-agent-tags.toml");
    if config_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            if let Ok(mut cfg) = toml::from_str::<Config>(&content) {
                // Merge user ignore patterns with defaults (additive).
                let defaults = default_ignore();
                for pattern in defaults {
                    if !cfg.ignore.contains(&pattern) {
                        cfg.ignore.push(pattern);
                    }
                }
                return cfg;
            }
        }
    }
    Config::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ignore_exact_component() {
        let patterns = vec!["node_modules".to_string(), "dist".to_string()];
        assert!(is_ignored("node_modules/lodash/index.js", &patterns));
        assert!(is_ignored("dist/bundle.js", &patterns));
        assert!(!is_ignored("src/dist-utils.ts", &patterns));
        assert!(!is_ignored("src/rebuild.rs", &["build".to_string()]));
    }

    #[test]
    fn test_ignore_glob_extension() {
        let patterns = vec!["*.test.ts".to_string(), "*.spec.ts".to_string()];
        assert!(is_ignored("src/auth.test.ts", &patterns));
        assert!(is_ignored("src/auth.spec.ts", &patterns));
        assert!(!is_ignored("src/auth.ts", &patterns));
    }

    #[test]
    fn test_ignore_nested_component() {
        let patterns = vec!["target".to_string()];
        assert!(is_ignored("git-agent-headers/target/debug/foo", &patterns));
        assert!(!is_ignored("git-agent-headers/src/target_dir.rs", &patterns));
    }

    #[test]
    fn test_ignore_double_star_glob() {
        let patterns = vec!["**/*.test.ts".to_string()];
        assert!(is_ignored("src/deep/nested/auth.test.ts", &patterns));
        assert!(!is_ignored("src/deep/nested/auth.ts", &patterns));
    }

    #[test]
    fn test_ignore_question_mark_glob() {
        let patterns = vec!["?.ts".to_string()];
        assert!(is_ignored("a.ts", &patterns));
        assert!(!is_ignored("ab.ts", &patterns));
    }

    #[test]
    fn test_ignore_is_additive_with_defaults() {
        // Simulate what load_config does when a user provides an ignore list
        let mut cfg = Config {
            ignore: vec!["*.test.ts".to_string()],
            ..Config::default()
        };
        let defaults = default_ignore();
        for pattern in defaults {
            if !cfg.ignore.contains(&pattern) {
                cfg.ignore.push(pattern);
            }
        }
        // User pattern preserved
        assert!(cfg.ignore.contains(&"*.test.ts".to_string()));
        // Defaults also present
        assert!(cfg.ignore.contains(&"node_modules".to_string()));
        assert!(cfg.ignore.contains(&"*.md".to_string()));
        // No duplicates
        let count = cfg.ignore.iter().filter(|p| p.as_str() == "*.md").count();
        assert_eq!(count, 1);
    }
}

/// Check if a file path should be ignored given the config patterns.
///
/// Pattern types:
/// - Plain name (e.g., `build`): matches any exact path component
/// - Glob with wildcards (e.g., `*.test.ts`, `**/*.spec.ts`): matched against
///   the full path using the `glob` crate's `Pattern`
pub fn is_ignored(path: &str, ignore_patterns: &[String]) -> bool {
    let components: Vec<&str> = path.split('/').collect();
    for pattern in ignore_patterns {
        if pattern.contains('*') || pattern.contains('?') || pattern.contains('[') {
            // Glob pattern — match against the full path
            if let Ok(pat) = glob::Pattern::new(pattern) {
                if pat.matches(path) {
                    return true;
                }
                // Also try matching against just the filename for simple globs like *.md
                if let Some(filename) = components.last() {
                    if pat.matches(filename) {
                        return true;
                    }
                }
            }
            continue;
        }
        // Plain name: match any path component exactly
        if components.iter().any(|c| *c == pattern.as_str()) {
            return true;
        }
    }
    false
}
