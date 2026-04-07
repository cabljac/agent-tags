/**
 * @agents
 * Range tag pairing and coverage computation.
 * Pairs start/end markers, computes file-level and line-level coverage.
 * Related: git-agent-tags/src/parser.rs, git-agent-tags/src/cache.rs
 */

use std::collections::{HashMap, HashSet};

use crate::cache::Index;
use crate::parser::{AgentsTag, RangeRole, TagKind};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RangePair {
    pub file: String,
    pub name: String,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone)]
pub struct UnmatchedRange {
    pub file: String,
    pub name: String,
    pub line: usize,
    pub role: RangeRole,
}

#[derive(Debug, Clone)]
pub struct FileCoverage {
    pub path: String,
    pub total_lines: usize,
    pub header_lines: usize,
    pub range_lines: usize,
    pub inline_lines: usize,
}

#[derive(Debug)]
pub struct CoverageSummary {
    pub total_files: usize,
    pub files_with_headers: usize,
    pub total_lines: usize,
    pub range_lines: usize,
    pub per_file: Vec<FileCoverage>,
    pub unmatched: Vec<UnmatchedRange>,
    pub uncovered_hotspots: Vec<(String, usize)>,
}

/// Pair start/end range tags by (file, name). Returns matched pairs and unmatched tags.
pub fn pair_range_tags(
    all_tags: &HashMap<String, Vec<AgentsTag>>,
) -> (Vec<RangePair>, Vec<UnmatchedRange>) {
    let mut pairs = Vec::new();
    let mut unmatched = Vec::new();

    // Group range tags by (file, name)
    let mut groups: HashMap<(String, String), Vec<(usize, RangeRole)>> = HashMap::new();

    for (file, tags) in all_tags {
        for tag in tags {
            if let (Some(name), Some(role)) = (&tag.name, &tag.range_role) {
                groups
                    .entry((file.clone(), name.clone()))
                    .or_default()
                    .push((tag.line, *role));
            }
        }
    }

    for ((file, name), mut entries) in groups {
        entries.sort_by_key(|(line, _)| *line);

        let mut pending_start: Option<usize> = None;

        for (line, role) in entries {
            match role {
                RangeRole::Start => {
                    if let Some(prev_line) = pending_start.take() {
                        unmatched.push(UnmatchedRange {
                            file: file.clone(),
                            name: name.clone(),
                            line: prev_line,
                            role: RangeRole::Start,
                        });
                    }
                    pending_start = Some(line);
                }
                RangeRole::End => {
                    if let Some(start_line) = pending_start.take() {
                        pairs.push(RangePair {
                            file: file.clone(),
                            name: name.clone(),
                            start_line,
                            end_line: line,
                        });
                    } else {
                        unmatched.push(UnmatchedRange {
                            file: file.clone(),
                            name: name.clone(),
                            line,
                            role: RangeRole::End,
                        });
                    }
                }
            }
        }

        if let Some(line) = pending_start {
            unmatched.push(UnmatchedRange {
                file: file.clone(),
                name: name.clone(),
                line,
                role: RangeRole::Start,
            });
        }
    }

    pairs.sort_by(|a, b| a.file.cmp(&b.file).then(a.start_line.cmp(&b.start_line)));
    unmatched.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));

    (pairs, unmatched)
}

/// Compute coverage metrics for all files.
pub fn compute_coverage(
    index: &Index,
    all_tags: &HashMap<String, Vec<AgentsTag>>,
    line_counts: &HashMap<String, usize>,
) -> CoverageSummary {
    let (pairs, unmatched) = pair_range_tags(all_tags);

    // Group pairs by file for efficient lookup
    let mut pairs_by_file: HashMap<&str, Vec<&RangePair>> = HashMap::new();
    for pair in &pairs {
        pairs_by_file.entry(&pair.file).or_default().push(pair);
    }

    let mut per_file = Vec::new();
    let mut total_lines: usize = 0;
    let mut range_lines_total: usize = 0;
    let mut files_with_headers: usize = 0;
    let mut files_with_any_tags: HashSet<&str> = HashSet::new();

    // Track which files have any tags
    for file in all_tags.keys() {
        if !all_tags[file].is_empty() {
            files_with_any_tags.insert(file);
        }
    }

    for (path, &lines) in line_counts {
        total_lines += lines;

        let cached = index.get(path);
        let header_lines = cached
            .and_then(|c| c.header.as_ref())
            .map(|h| h.end_line.saturating_sub(h.start_line) + 1)
            .unwrap_or(0);

        if cached.map_or(false, |c| c.has_header) {
            files_with_headers += 1;
        }

        // Range lines: deduplicate via HashSet for overlapping different-name ranges
        let range_lines = if let Some(file_pairs) = pairs_by_file.get(path.as_str()) {
            let mut covered: HashSet<usize> = HashSet::new();
            for pair in file_pairs {
                for line in pair.start_line..=pair.end_line {
                    covered.insert(line);
                }
            }
            covered.len()
        } else {
            0
        };

        // Inline lines: standalone inline tags (no range_role)
        let inline_lines = all_tags
            .get(path)
            .map(|tags| {
                tags.iter()
                    .filter(|t| t.kind == TagKind::Inline && t.range_role.is_none())
                    .map(|t| t.text.len())
                    .sum()
            })
            .unwrap_or(0);

        range_lines_total += range_lines;

        per_file.push(FileCoverage {
            path: path.clone(),
            total_lines: lines,
            header_lines,
            range_lines,
            inline_lines,
        });
    }

    // Uncovered hotspots: files with no tags at all, sorted by size
    let mut uncovered_hotspots: Vec<(String, usize)> = line_counts
        .iter()
        .filter(|(path, _)| !files_with_any_tags.contains(path.as_str()))
        .map(|(path, &lines)| (path.clone(), lines))
        .collect();
    uncovered_hotspots.sort_by(|a, b| b.1.cmp(&a.1));

    CoverageSummary {
        total_files: line_counts.len(),
        files_with_headers,
        total_lines,
        range_lines: range_lines_total,
        per_file,
        unmatched,
        uncovered_hotspots,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{CachedFile, CachedHeader};

    fn make_tag(
        file: &str,
        name: Option<&str>,
        line: usize,
        role: Option<RangeRole>,
        kind: TagKind,
        text: &[&str],
    ) -> AgentsTag {
        AgentsTag {
            file: file.to_string(),
            name: name.map(|s| s.to_string()),
            range_role: role,
            line,
            text: text.iter().map(|s| s.to_string()).collect(),
            kind,
        }
    }

    #[test]
    fn test_pair_simple_range() {
        let mut tags: HashMap<String, Vec<AgentsTag>> = HashMap::new();
        tags.insert(
            "auth.rs".to_string(),
            vec![
                make_tag("auth.rs", Some("validate"), 10, Some(RangeRole::Start), TagKind::Inline, &["Check input."]),
                make_tag("auth.rs", Some("validate"), 20, Some(RangeRole::End), TagKind::Inline, &[""]),
            ],
        );

        let (pairs, unmatched) = pair_range_tags(&tags);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].start_line, 10);
        assert_eq!(pairs[0].end_line, 20);
        assert!(unmatched.is_empty());
    }

    #[test]
    fn test_unmatched_start() {
        let mut tags: HashMap<String, Vec<AgentsTag>> = HashMap::new();
        tags.insert(
            "auth.rs".to_string(),
            vec![make_tag("auth.rs", Some("validate"), 10, Some(RangeRole::Start), TagKind::Inline, &["Check."])],
        );

        let (pairs, unmatched) = pair_range_tags(&tags);
        assert!(pairs.is_empty());
        assert_eq!(unmatched.len(), 1);
        assert_eq!(unmatched[0].role, RangeRole::Start);
        assert_eq!(unmatched[0].line, 10);
    }

    #[test]
    fn test_unmatched_end() {
        let mut tags: HashMap<String, Vec<AgentsTag>> = HashMap::new();
        tags.insert(
            "auth.rs".to_string(),
            vec![make_tag("auth.rs", Some("validate"), 20, Some(RangeRole::End), TagKind::Inline, &[""])],
        );

        let (pairs, unmatched) = pair_range_tags(&tags);
        assert!(pairs.is_empty());
        assert_eq!(unmatched.len(), 1);
        assert_eq!(unmatched[0].role, RangeRole::End);
    }

    #[test]
    fn test_multiple_pairs_same_name() {
        let mut tags: HashMap<String, Vec<AgentsTag>> = HashMap::new();
        tags.insert(
            "auth.rs".to_string(),
            vec![
                make_tag("auth.rs", Some("check"), 5, Some(RangeRole::Start), TagKind::Inline, &["First."]),
                make_tag("auth.rs", Some("check"), 10, Some(RangeRole::End), TagKind::Inline, &[""]),
                make_tag("auth.rs", Some("check"), 20, Some(RangeRole::Start), TagKind::Inline, &["Second."]),
                make_tag("auth.rs", Some("check"), 30, Some(RangeRole::End), TagKind::Inline, &[""]),
            ],
        );

        let (pairs, unmatched) = pair_range_tags(&tags);
        assert_eq!(pairs.len(), 2);
        assert!(unmatched.is_empty());
        assert_eq!(pairs[0].start_line, 5);
        assert_eq!(pairs[0].end_line, 10);
        assert_eq!(pairs[1].start_line, 20);
        assert_eq!(pairs[1].end_line, 30);
    }

    #[test]
    fn test_double_start_flags_first_unmatched() {
        let mut tags: HashMap<String, Vec<AgentsTag>> = HashMap::new();
        tags.insert(
            "auth.rs".to_string(),
            vec![
                make_tag("auth.rs", Some("check"), 5, Some(RangeRole::Start), TagKind::Inline, &["First."]),
                make_tag("auth.rs", Some("check"), 10, Some(RangeRole::Start), TagKind::Inline, &["Second."]),
                make_tag("auth.rs", Some("check"), 20, Some(RangeRole::End), TagKind::Inline, &[""]),
            ],
        );

        let (pairs, unmatched) = pair_range_tags(&tags);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].start_line, 10);
        assert_eq!(pairs[0].end_line, 20);
        assert_eq!(unmatched.len(), 1);
        assert_eq!(unmatched[0].line, 5);
        assert_eq!(unmatched[0].role, RangeRole::Start);
    }

    #[test]
    fn test_different_names_independent() {
        let mut tags: HashMap<String, Vec<AgentsTag>> = HashMap::new();
        tags.insert(
            "auth.rs".to_string(),
            vec![
                make_tag("auth.rs", Some("auth"), 5, Some(RangeRole::Start), TagKind::Inline, &["Auth."]),
                make_tag("auth.rs", Some("logging"), 8, Some(RangeRole::Start), TagKind::Inline, &["Log."]),
                make_tag("auth.rs", Some("auth"), 15, Some(RangeRole::End), TagKind::Inline, &[""]),
                make_tag("auth.rs", Some("logging"), 20, Some(RangeRole::End), TagKind::Inline, &[""]),
            ],
        );

        let (pairs, unmatched) = pair_range_tags(&tags);
        assert_eq!(pairs.len(), 2);
        assert!(unmatched.is_empty());
    }

    #[test]
    fn test_compute_coverage_basic() {
        let mut index = Index::new();
        index.upsert(CachedFile {
            path: "src/auth.rs".to_string(),
            has_header: true,
            header: Some(CachedHeader {
                name: None,
                body: vec!["Auth module.".to_string()],
                related: vec![],
                see: vec![],
                warnings: vec![],
                start_line: 1,
                end_line: 4,
                last_header_commit: None,
            }),
            mtime_secs: None,
            file_size: None,
            tag_names: vec![],
        });
        index.upsert(CachedFile {
            path: "src/main.rs".to_string(),
            has_header: false,
            header: None,
            mtime_secs: None,
            file_size: None,
            tag_names: vec![],
        });

        let mut all_tags: HashMap<String, Vec<AgentsTag>> = HashMap::new();
        all_tags.insert(
            "src/auth.rs".to_string(),
            vec![
                make_tag("src/auth.rs", None, 1, None, TagKind::FileHeader, &["Auth module."]),
                make_tag("src/auth.rs", Some("validate"), 10, Some(RangeRole::Start), TagKind::Inline, &["Check."]),
                make_tag("src/auth.rs", Some("validate"), 20, Some(RangeRole::End), TagKind::Inline, &[""]),
            ],
        );

        let mut line_counts: HashMap<String, usize> = HashMap::new();
        line_counts.insert("src/auth.rs".to_string(), 50);
        line_counts.insert("src/main.rs".to_string(), 100);

        let summary = compute_coverage(&index, &all_tags, &line_counts);
        assert_eq!(summary.total_files, 2);
        assert_eq!(summary.files_with_headers, 1);
        assert_eq!(summary.total_lines, 150);
        assert_eq!(summary.range_lines, 11); // lines 10..=20
        assert!(summary.unmatched.is_empty());
        assert_eq!(summary.uncovered_hotspots.len(), 1);
        assert_eq!(summary.uncovered_hotspots[0].0, "src/main.rs");
    }

    #[test]
    fn test_overlapping_different_names_deduped() {
        let index = Index::new();
        let mut all_tags: HashMap<String, Vec<AgentsTag>> = HashMap::new();
        all_tags.insert(
            "auth.rs".to_string(),
            vec![
                make_tag("auth.rs", Some("auth"), 10, Some(RangeRole::Start), TagKind::Inline, &["Auth."]),
                make_tag("auth.rs", Some("logging"), 15, Some(RangeRole::Start), TagKind::Inline, &["Log."]),
                make_tag("auth.rs", Some("auth"), 20, Some(RangeRole::End), TagKind::Inline, &[""]),
                make_tag("auth.rs", Some("logging"), 25, Some(RangeRole::End), TagKind::Inline, &[""]),
            ],
        );

        let mut line_counts: HashMap<String, usize> = HashMap::new();
        line_counts.insert("auth.rs".to_string(), 30);

        let summary = compute_coverage(&index, &all_tags, &line_counts);
        // auth: 10..=20 (11 lines), logging: 15..=25 (11 lines), overlap 15..=20 (6 lines)
        // Deduped: 10..=25 = 16 lines
        assert_eq!(summary.range_lines, 16);
    }
}
