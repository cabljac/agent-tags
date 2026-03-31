/**
 * @agents
 * Git history queries via git2: blame, diff stats, rename detection.
 * Powers Tier 1 staleness detection in check.rs.
 * Related: git-agent-headers/src/check.rs, git-agent-headers/src/cache.rs
 */

use anyhow::{Context, Result};
use git2::{DiffOptions, Repository};
use std::collections::HashMap;
use std::path::Path;

pub struct GitRepo {
    repo: Repository,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct StalenessInfo {
    /// Last commit SHA that touched the file
    pub last_file_commit: Option<String>,
    /// Last commit SHA that touched the header lines
    pub last_header_commit: Option<String>,
    /// Number of commits since the header was last updated
    pub commit_gap: usize,
    /// Percentage of lines changed since the header commit
    pub diff_percent: f64,
    /// Whether the file was renamed (old_path → new_path)
    pub renamed_from: Option<String>,
}

#[derive(Debug)]
pub struct RenameInfo {
    pub old_path: String,
    pub new_path: String,
    #[allow(dead_code)]
    pub commit: String,
}

impl GitRepo {
    pub fn open(path: &Path) -> Result<Self> {
        let repo = Repository::discover(path)
            .with_context(|| format!("Not a git repository: {}", path.display()))?;
        Ok(Self { repo })
    }

    pub fn workdir(&self) -> Option<&Path> {
        self.repo.workdir()
    }

    pub fn git_dir(&self) -> &Path {
        self.repo.path()
    }

    /// SHA of the most recent commit that changed lines in the given range.
    pub fn last_commit_for_lines(
        &self,
        file: &str,
        start_line: usize,
        end_line: usize,
    ) -> Result<Option<String>> {
        let blame = match self.repo.blame_file(Path::new(file), None) {
            Ok(b) => b,
            Err(_) => return Ok(None),
        };

        let mut newest_time: Option<(i64, String)> = None;
        for hunk in blame.iter() {
            let hunk_start = hunk.final_start_line();
            let hunk_end = hunk_start + hunk.lines_in_hunk();
            // Check for overlap
            if hunk_start <= end_line && hunk_end >= start_line {
                let sig = hunk.final_signature();
                let time = sig.when().seconds();
                let sha = hunk.final_commit_id().to_string();
                if newest_time.as_ref().map_or(true, |(t, _)| time > *t) {
                    newest_time = Some((time, sha));
                }
            }
        }

        Ok(newest_time.map(|(_, sha)| sha))
    }

    /// Combined: returns (last_file_commit, commit_gap_since_header) in one revwalk.
    /// More efficient than calling last_commit_for_file + commit_gap separately.
    pub fn file_staleness_counts(
        &self,
        file: &str,
        header_sha: &str,
    ) -> Result<(Option<String>, usize)> {
        let header_oid = match git2::Oid::from_str(header_sha) {
            Ok(o) => o,
            Err(_) => return Ok((None, 0)),
        };

        let mut revwalk = self.repo.revwalk()?;
        revwalk.push_head().ok();
        revwalk.set_sorting(git2::Sort::TIME)?;

        let mut last_commit: Option<String> = None;
        let mut gap = 0;
        for oid in revwalk {
            let oid = oid?;
            if oid == header_oid {
                break;
            }
            let commit = self.repo.find_commit(oid)?;
            if self.commit_touches_file(&commit, file)? {
                if last_commit.is_none() {
                    last_commit = Some(oid.to_string());
                }
                gap += 1;
            }
        }
        Ok((last_commit, gap))
    }

    /// Percentage of lines changed between old_sha and HEAD for a file.
    pub fn diff_percent_since(&self, old_sha: &str, file: &str) -> Result<f64> {
        let old_oid = match git2::Oid::from_str(old_sha) {
            Ok(o) => o,
            Err(_) => return Ok(0.0),
        };

        let old_commit = self.repo.find_commit(old_oid)?;
        let old_tree = old_commit.tree()?;

        let head = match self.repo.head() {
            Ok(h) => h,
            Err(_) => return Ok(0.0),
        };
        let head_commit = head.peel_to_commit()?;
        let head_tree = head_commit.tree()?;

        let mut opts = DiffOptions::new();
        opts.pathspec(file);

        let diff = self
            .repo
            .diff_tree_to_tree(Some(&old_tree), Some(&head_tree), Some(&mut opts))?;

        let stats = diff.stats()?;
        let total_lines = stats.insertions() + stats.deletions();

        // Get total lines in the old file for percentage calculation
        let old_entry = old_tree.get_path(Path::new(file)).ok();
        let old_lines = if let Some(entry) = old_entry {
            if let Ok(blob) = self.repo.find_blob(entry.id()) {
                blob.content()
                    .iter()
                    .filter(|&&b| b == b'\n')
                    .count()
                    .max(1)
            } else {
                1
            }
        } else {
            1
        };

        Ok((total_lines as f64 / old_lines as f64) * 100.0)
    }

    /// Detect file renames in recent history.
    pub fn detect_renames(&self, limit: usize) -> Result<Vec<RenameInfo>> {
        let mut renames = Vec::new();
        let mut revwalk = self.repo.revwalk()?;
        revwalk.push_head().ok();
        revwalk.set_sorting(git2::Sort::TIME)?;

        let mut count = 0;
        for oid in revwalk {
            if count >= limit {
                break;
            }
            let oid = oid?;
            let commit = self.repo.find_commit(oid)?;
            if commit.parent_count() == 0 {
                count += 1;
                continue;
            }
            let parent = commit.parent(0)?;
            let old_tree = parent.tree()?;
            let new_tree = commit.tree()?;

            let mut opts = DiffOptions::new();
            let mut diff =
                self.repo
                    .diff_tree_to_tree(Some(&old_tree), Some(&new_tree), Some(&mut opts))?;

            diff.find_similar(None)?;

            diff.foreach(
                &mut |_, _| true,
                None,
                None,
                Some(&mut |delta, _, _| {
                    if delta.status() == git2::Delta::Renamed {
                        if let (Some(old), Some(new)) = (delta.old_file().path(), delta.new_file().path()) {
                            renames.push(RenameInfo {
                                old_path: old.to_string_lossy().to_string(),
                                new_path: new.to_string_lossy().to_string(),
                                commit: oid.to_string(),
                            });
                        }
                    }
                    true
                }),
            )?;

            count += 1;
        }
        Ok(renames)
    }

    /// Get diff text since a given commit for a file.
    pub fn diff_since(&self, old_sha: &str, file: &str) -> Result<String> {
        let old_oid = match git2::Oid::from_str(old_sha) {
            Ok(o) => o,
            Err(_) => return Ok(String::new()),
        };

        let old_commit = self.repo.find_commit(old_oid)?;
        let old_tree = old_commit.tree()?;

        let head = match self.repo.head() {
            Ok(h) => h,
            Err(_) => return Ok(String::new()),
        };
        let head_commit = head.peel_to_commit()?;
        let head_tree = head_commit.tree()?;

        let mut opts = DiffOptions::new();
        opts.pathspec(file);

        let diff =
            self.repo
                .diff_tree_to_tree(Some(&old_tree), Some(&head_tree), Some(&mut opts))?;

        let mut out = Vec::new();
        diff.print(git2::DiffFormat::Patch, |_, _, line| {
            out.extend_from_slice(line.content());
            true
        })?;

        Ok(String::from_utf8_lossy(&out).to_string())
    }

    /// Co-change analysis: returns map of (file_a, file_b) → commit count.
    pub fn cochange_counts(
        &self,
        limit_commits: usize,
        max_files_per_commit: usize,
    ) -> Result<HashMap<(String, String), usize>> {
        let mut counts: HashMap<(String, String), usize> = HashMap::new();
        let mut revwalk = self.repo.revwalk()?;
        revwalk.push_head().ok();
        revwalk.set_sorting(git2::Sort::TIME)?;

        let noisy_files = ["package.json", "Cargo.toml", "Cargo.lock", "package-lock.json", "yarn.lock", "pnpm-lock.yaml"];

        let mut processed = 0;
        for oid in revwalk {
            if processed >= limit_commits {
                break;
            }
            let oid = oid?;
            let commit = self.repo.find_commit(oid)?;
            if commit.parent_count() == 0 {
                processed += 1;
                continue;
            }
            let parent = commit.parent(0)?;
            let old_tree = parent.tree()?;
            let new_tree = commit.tree()?;

            let diff = self
                .repo
                .diff_tree_to_tree(Some(&old_tree), Some(&new_tree), None)?;

            let mut files: Vec<String> = Vec::new();
            diff.foreach(
                &mut |delta, _| {
                    if let Some(path) = delta.new_file().path() {
                        let p = path.to_string_lossy().to_string();
                        if !noisy_files.iter().any(|&n| p.ends_with(n)) {
                            files.push(p);
                        }
                    }
                    true
                },
                None,
                None,
                None,
            )?;

            if files.len() > max_files_per_commit {
                processed += 1;
                continue;
            }

            // Count all pairs
            for i in 0..files.len() {
                for j in (i + 1)..files.len() {
                    let pair = if files[i] < files[j] {
                        (files[i].clone(), files[j].clone())
                    } else {
                        (files[j].clone(), files[i].clone())
                    };
                    *counts.entry(pair).or_insert(0) += 1;
                }
            }

            processed += 1;
        }
        Ok(counts)
    }

    fn commit_touches_file(&self, commit: &git2::Commit, file: &str) -> Result<bool> {
        if commit.parent_count() == 0 {
            // Root commit: check if file exists in tree
            let tree = commit.tree()?;
            return Ok(tree.get_path(Path::new(file)).is_ok());
        }
        let parent = commit.parent(0)?;
        let old_tree = parent.tree()?;
        let new_tree = commit.tree()?;

        let mut opts = DiffOptions::new();
        opts.pathspec(file);
        let diff =
            self.repo
                .diff_tree_to_tree(Some(&old_tree), Some(&new_tree), Some(&mut opts))?;

        Ok(diff.deltas().count() > 0)
    }
}
