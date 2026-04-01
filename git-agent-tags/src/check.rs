/**
 * @agents
 * Warning/detection logic combining parser, graph, and git heuristics.
 * Produces tiered output: broken refs, stale headers, suggestions.
 * Related: git-agent-tags/src/parser.rs, git-agent-tags/src/graph.rs, git-agent-tags/src/git.rs, git-agent-tags/src/cache.rs
 */

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use anyhow::Result;
use regex::Regex;

use crate::cache::Index;
use crate::git::GitRepo;
use crate::graph::ReferenceGraph;

#[derive(Debug, Clone)]
pub enum WarnLevel {
    /// ✗ broken reference
    Broken,
    /// ⚠ likely stale
    Stale,
    /// ℹ suggestion
    Info,
}

#[derive(Debug, Clone)]
pub struct Warning {
    pub file: String,
    pub level: WarnLevel,
    pub message: String,
}

/// Tier 1: Git heuristics (staleness via commit gap + diff percent).
/// When `lines_owned` is set, staleness is scoped to the owned line range
/// (starting after `header_end`) rather than the whole file.
pub fn check_git_staleness(
    file: &str,
    header_start: usize,
    header_end: usize,
    lines_owned: Option<usize>,
    repo: &GitRepo,
    stale_commit_gap: usize,
    stale_diff_percent: f64,
) -> Result<Vec<Warning>> {
    let mut warnings = Vec::new();

    let header_sha = match repo.last_commit_for_lines(file, header_start, header_end)? {
        Some(s) => s,
        None => return Ok(warnings),
    };

    // When lines_owned is set, only check the owned range — skip whole-file heuristics.
    if let Some(owned) = lines_owned {
        let owned_start = header_end + 1;
        let owned_end = header_end + owned;
        if let Some(owned_sha) = repo.last_commit_for_lines(file, owned_start, owned_end)? {
            if owned_sha != header_sha {
                warnings.push(Warning {
                    file: file.to_string(),
                    level: WarnLevel::Stale,
                    message: format!(
                        "owned lines {}-{} changed since tag last updated",
                        owned_start, owned_end
                    ),
                });
            }
        }
        return Ok(warnings);
    }

    // Whole-file heuristics (no lines_owned).
    let (last_file_commit, gap) = repo.file_staleness_counts(file, &header_sha)?;

    // No changes since the header commit.
    if last_file_commit.is_none() {
        return Ok(warnings);
    }

    if gap > stale_commit_gap {
        warnings.push(Warning {
            file: file.to_string(),
            level: WarnLevel::Stale,
            message: format!(
                "header not updated in {} commits (threshold: {})",
                gap, stale_commit_gap
            ),
        });
    }

    let pct = repo.diff_percent_since(&header_sha, file)?;
    if pct > stale_diff_percent {
        let pct_display = if pct > 100.0 {
            "100%+".to_string()
        } else {
            format!("{:.0}%", pct)
        };
        warnings.push(Warning {
            file: file.to_string(),
            level: WarnLevel::Stale,
            message: format!(
                "{} of file changed since header last updated (threshold: {:.0}%)",
                pct_display, stale_diff_percent
            ),
        });
    }

    Ok(warnings)
}

static EXPORT_RE: OnceLock<Regex> = OnceLock::new();
static IMPORT_RE: OnceLock<Regex> = OnceLock::new();

fn export_re() -> &'static Regex {
    EXPORT_RE.get_or_init(|| {
        Regex::new(
            r"^\+.*\b(export\s+(function|const|class|type|interface|default)|module\.exports|pub\s+fn|pub\s+struct|def\s+\w|func\s+\w)",
        )
        .unwrap()
    })
}

fn import_re() -> &'static Regex {
    IMPORT_RE.get_or_init(|| {
        Regex::new(r#"^\+.*\b(import\s+.*\s+from\s+['"]([^'"]+)['"]|require\(['"]([^'"]+)['"]\))"#)
            .unwrap()
    })
}

/// Tier 2: Regex heuristics on the diff since the header commit.
pub fn check_regex_staleness(
    file: &str,
    header_sha: &str,
    current_related: &[String],
    repo: &GitRepo,
) -> Result<Vec<Warning>> {
    let mut warnings = Vec::new();
    let diff = repo.diff_since(header_sha, file)?;
    if diff.is_empty() {
        return Ok(warnings);
    }

    let export_re = export_re();
    let import_re = import_re();

    let mut new_exports = false;
    let mut new_imports: Vec<String> = Vec::new();

    for line in diff.lines() {
        if export_re.is_match(line) {
            new_exports = true;
        }
        if let Some(caps) = import_re.captures(line) {
            let path = caps
                .get(2)
                .or_else(|| caps.get(3))
                .map(|m| m.as_str().to_string());
            if let Some(p) = path {
                // Relative import that isn't in Related:
                if p.starts_with('.') && !current_related.iter().any(|r| r.contains(&p)) {
                    new_imports.push(p);
                }
            }
        }
    }

    if new_exports {
        warnings.push(Warning {
            file: file.to_string(),
            level: WarnLevel::Stale,
            message: "new exports added since header last updated — consider updating the header"
                .to_string(),
        });
    }

    for imp in &new_imports {
        warnings.push(Warning {
            file: file.to_string(),
            level: WarnLevel::Info,
            message: format!(
                "new import from '{}' not mentioned in Related:",
                imp
            ),
        });
    }

    Ok(warnings)
}

/// Build co-change suggestions: pairs that frequently co-change but don't reference each other.
pub fn cochange_suggestions(
    repo: &GitRepo,
    index: &Index,
    graph: &ReferenceGraph,
    min_commits: usize,
    max_files_per_commit: usize,
) -> Result<Vec<Warning>> {
    let mut warnings = Vec::new();
    let counts = repo.cochange_counts(500, max_files_per_commit)?;

    let files_with_headers: HashSet<String> = index
        .files_with_headers()
        .iter()
        .map(|f| f.path.clone())
        .collect();

    for ((a, b), count) in &counts {
        if *count < min_commits {
            continue;
        }
        // Only suggest for files that have headers
        if !files_with_headers.contains(a) || !files_with_headers.contains(b) {
            continue;
        }
        // Check if they already reference each other
        let deps_a = graph.dependencies(a);
        let deps_b = graph.dependencies(b);
        if deps_a.contains(b) || deps_b.contains(a) {
            continue;
        }
        warnings.push(Warning {
            file: a.clone(),
            level: WarnLevel::Info,
            message: format!(
                "frequently co-changes with {} ({} commits) — consider adding Related:",
                b, count
            ),
        });
    }
    Ok(warnings)
}

/// Check for broken references using rename detection.
pub fn check_renames(graph: &ReferenceGraph, repo: &GitRepo) -> Result<Vec<Warning>> {
    let mut warnings = Vec::new();
    let renames = repo.detect_renames(200)?;

    let rename_map: HashMap<String, String> = renames
        .into_iter()
        .map(|r| (r.old_path, r.new_path))
        .collect();

    for file in graph.all_files() {
        let deps = graph.dependencies(file);
        for dep in deps {
            if let Some(new_path) = rename_map.get(&dep) {
                warnings.push(Warning {
                    file: file.to_string(),
                    level: WarnLevel::Broken,
                    message: format!(
                        "Related: {} (renamed to {})",
                        dep, new_path
                    ),
                });
            }
        }
    }
    Ok(warnings)
}
