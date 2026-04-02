/**
 * @agents
 * CLI entry point. Wires up clap commands to the underlying modules.
 * Each subcommand delegates to parser, graph, git, check, or cache.
 * Related: git-agent-tags/src/parser.rs, git-agent-tags/src/graph.rs, git-agent-tags/src/git.rs, git-agent-tags/src/check.rs, git-agent-tags/src/cache.rs, git-agent-tags/src/config.rs
 */

mod cache;
mod check;
mod config;
mod git;
mod graph;
mod parser;

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use walkdir::WalkDir;

use cache::{cached_header_from_block, CachedFile, Index};
use check::{WarnLevel, Warning};
use config::{is_ignored, load_config, Config};
use git::GitRepo;
use graph::{GraphNode, ReferenceGraph};
use parser::{AgentsTag, TagKind};

#[derive(Parser)]
#[command(
    name = "git-agent-tags",
    about = "Parse and validate @agents tags in codebases",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse all files, build the reference graph, report overall status
    Status,
    /// Show stale headers and broken references
    Check {
        /// Also run regex-based heuristics (slightly slower)
        #[arg(long)]
        deep: bool,
    },
    /// Show only broken references (Related: pointing to missing files)
    Broken,
    /// Show files missing @agents headers
    Missing,
    /// Suggest Related: links based on co-change history
    Suggest,
    /// Show the reference graph for a file
    Graph {
        file: String,
    },
    /// Rebuild the file index cache
    Reindex,
    /// Print all @agents tags across the repo to stdout
    Context,
    /// Run as a pre-commit hook: fail on broken refs, warn on staleness
    Hook {
        /// Also run regex-based heuristics
        #[arg(long)]
        deep: bool,
        /// Install the pre-commit hook into .git/hooks
        #[arg(long)]
        install: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let repo = GitRepo::open(Path::new("."))?;
    let workdir = repo
        .workdir()
        .context("Bare repositories are not supported")?
        .to_path_buf();
    let git_dir = repo.git_dir().to_path_buf();
    let config = load_config(&workdir);

    match cli.command {
        Command::Status => cmd_status(&workdir, &git_dir, &config, &repo),
        Command::Check { deep } => cmd_check(&workdir, &git_dir, &config, &repo, deep),
        Command::Broken => cmd_broken(&workdir, &git_dir, &config),
        Command::Missing => cmd_missing(&workdir, &git_dir, &config),
        Command::Suggest => cmd_suggest(&workdir, &git_dir, &config, &repo),
        Command::Graph { file } => cmd_graph(&workdir, &git_dir, &config, &file),
        Command::Reindex => cmd_reindex(&workdir, &git_dir, &config),
        Command::Context => cmd_build(&workdir, &git_dir, &config),
        Command::Hook { deep, install } => {
            if install {
                cmd_hook_install(&git_dir)
            } else {
                cmd_hook(&workdir, &git_dir, &config, &repo, deep)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn populate_header_commits(index: &mut Index, repo: &GitRepo) {
    for cached in index.files_with_headers_mut() {
        if let Some(header) = &mut cached.header {
            if header.last_header_commit.is_none() {
                header.last_header_commit = repo
                    .last_commit_for_lines(&cached.path, header.start_line, header.end_line)
                    .ok()
                    .flatten();
            }
        }
    }
}

fn build_index_and_graph(
    workdir: &Path,
    git_dir: &Path,
    config: &Config,
) -> Result<(Index, ReferenceGraph, HashSet<String>)> {
    let prev_index = cache::load_index(git_dir).unwrap_or_default();
    let mut index = Index::new();
    let mut graph = ReferenceGraph::new();
    let mut all_files: HashSet<String> = HashSet::new();

    for entry in WalkDir::new(workdir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let abs = entry.path();
        let rel = abs
            .strip_prefix(workdir)
            .unwrap_or(abs)
            .to_string_lossy()
            .to_string();

        if is_ignored(&rel, &config.ignore) {
            continue;
        }

        all_files.insert(rel.clone());

        // Check if we can reuse the cached entry (mtime + size match).
        let meta = fs::metadata(abs).ok();
        let cur_mtime = meta.as_ref().and_then(|m| {
            m.modified().ok().and_then(|t| {
                t.duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .map(|d| d.as_secs() as i64)
            })
        });
        let cur_size = meta.as_ref().map(|m| m.len());

        if let Some(cached) = prev_index.get(&rel) {
            if cached.mtime_secs.is_some()
                && cached.mtime_secs == cur_mtime
                && cached.file_size == cur_size
            {
                // Reuse cached data — reconstruct graph node if it had a header.
                if cached.has_header {
                    if let Some(header) = &cached.header {
                        graph.add_node(GraphNode {
                            file: rel.clone(),
                            related: header.related.clone(),
                            see: header.see.clone(),
                        });
                    }
                }
                if !cached.tag_names.is_empty() {
                    graph.register_tag_names(
                        &rel,
                        cached.tag_names.iter().cloned().collect(),
                    );
                }
                index.upsert(cached.clone());
                continue;
            }
        }

        let content = match fs::read_to_string(abs) {
            Ok(c) => c,
            Err(_) => continue, // binary or unreadable
        };

        // Parse all tags to collect named tag names for fragment validation.
        let all_tags = parser::parse_all_agents_tags(&content, abs);
        let mut tag_name_set: HashSet<String> = HashSet::new();
        for tag in &all_tags {
            if let Some(name) = &tag.name {
                tag_name_set.insert(name.clone());
            }
        }
        let tag_names_vec: Vec<String> = tag_name_set.iter().cloned().collect();
        if !tag_name_set.is_empty() {
            graph.register_tag_names(&rel, tag_name_set);
        }

        if let Some(block) = parser::parse_agents_block(&content, abs) {
            graph.add_node(GraphNode {
                file: rel.clone(),
                related: block.related.clone(),
                see: block.see.clone(),
            });
            let cached = cached_header_from_block(&block);
            index.upsert(CachedFile {
                path: rel,
                has_header: true,
                header: Some(cached),
                mtime_secs: cur_mtime,
                file_size: cur_size,
                tag_names: tag_names_vec,
            });
        } else {
            index.upsert(CachedFile {
                path: rel,
                has_header: false,
                header: None,
                mtime_secs: cur_mtime,
                file_size: cur_size,
                tag_names: tag_names_vec,
            });
        }
    }

    Ok((index, graph, all_files))
}

fn print_warning(w: &Warning) {
    let prefix = match w.level {
        WarnLevel::Broken => "✗".red().bold(),
        WarnLevel::Stale => "⚠".yellow().bold(),
        WarnLevel::Info => "ℹ".cyan(),
    };
    println!("  {} {} — {}", prefix, w.file.bold(), w.message);
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

fn cmd_status(
    workdir: &Path,
    git_dir: &Path,
    config: &Config,
    _repo: &GitRepo,
) -> Result<()> {
    let (index, graph, all_files) = build_index_and_graph(workdir, git_dir, config)?;

    let total = all_files.len();
    let with_headers = index.files_with_headers().len();
    let missing = index.files_missing_headers().len();
    let broken = graph.broken_refs(&all_files);

    println!("{}", "git-agent-tags status".bold());
    println!("  Files scanned:       {}", total);
    println!("  With @agents header: {}", with_headers.to_string().green());
    println!("  Missing header:      {}", missing.to_string().yellow());
    println!(
        "  Broken references:   {}",
        if broken.is_empty() {
            "0".green()
        } else {
            broken.len().to_string().red()
        }
    );

    if !broken.is_empty() {
        println!("\nBroken references:");
        for (src, dep) in &broken {
            println!("  {} {} → {}", "✗".red(), src, dep);
        }
    }

    cache::save_index(git_dir, &index)?;
    Ok(())
}

fn cmd_check(
    workdir: &Path,
    git_dir: &Path,
    config: &Config,
    repo: &GitRepo,
    deep: bool,
) -> Result<()> {
    let (mut index, graph, all_files) = build_index_and_graph(workdir, git_dir, config)?;

    if deep {
        populate_header_commits(&mut index, repo);
    }

    cache::save_index(git_dir, &index)?;

    let mut all_warnings: Vec<Warning> = Vec::new();

    // Broken refs from graph
    for (src, dep) in graph.broken_refs(&all_files) {
        all_warnings.push(Warning {
            file: src,
            level: WarnLevel::Broken,
            message: format!("Related: {} (file not found)", dep),
        });
    }

    // Rename-based broken refs
    let rename_warnings = check::check_renames(&graph, repo)?;
    all_warnings.extend(rename_warnings);

    // Tier 1: git staleness for files with headers
    for cached in index.files_with_headers() {
        if let Some(header) = &cached.header {
            let warnings = check::check_git_staleness(
                &cached.path,
                header.start_line,
                header.end_line,
                header.lines_owned,
                repo,
                config.stale_commit_gap,
                config.stale_diff_percent,
            )?;
            all_warnings.extend(warnings);

            if deep {
                if let Some(sha) = &header.last_header_commit {
                    let w =
                        check::check_regex_staleness(&cached.path, sha, &header.related, repo)?;
                    all_warnings.extend(w);
                }
            }
        }
    }

    if all_warnings.is_empty() {
        println!("{}", "✓ No issues found.".green());
    } else {
        println!("{} issue(s) found:\n", all_warnings.len());
        for w in &all_warnings {
            print_warning(w);
        }
    }

    Ok(())
}

fn cmd_broken(workdir: &Path, git_dir: &Path, config: &Config) -> Result<()> {
    let (index, graph, all_files) = build_index_and_graph(workdir, git_dir, config)?;
    cache::save_index(git_dir, &index)?;

    let broken = graph.broken_refs(&all_files);
    if broken.is_empty() {
        println!("{}", "✓ No broken references.".green());
    } else {
        println!("{} broken reference(s):\n", broken.len());
        for (src, dep) in &broken {
            println!(
                "  {} {} — Related: {} (file not found)",
                "✗".red(),
                src.bold(),
                dep
            );
        }
    }
    Ok(())
}

fn cmd_missing(workdir: &Path, git_dir: &Path, config: &Config) -> Result<()> {
    let (index, _graph, _all_files) = build_index_and_graph(workdir, git_dir, config)?;
    cache::save_index(git_dir, &index)?;

    let missing = index.files_missing_headers();
    if missing.is_empty() {
        println!("{}", "✓ All files have @agents headers.".green());
    } else {
        println!("{} file(s) missing @agents headers:\n", missing.len());
        for f in &missing {
            println!("  {}", f.path);
        }
    }
    Ok(())
}

fn cmd_suggest(
    workdir: &Path,
    git_dir: &Path,
    config: &Config,
    repo: &GitRepo,
) -> Result<()> {
    let (index, graph, _all_files) = build_index_and_graph(workdir, git_dir, config)?;
    cache::save_index(git_dir, &index)?;

    let suggestions = check::cochange_suggestions(
        repo,
        &index,
        &graph,
        config.cochange_min_commits,
        config.cochange_max_files,
    )?;

    if suggestions.is_empty() {
        println!("{}", "✓ No co-change suggestions.".green());
    } else {
        println!("{} suggestion(s):\n", suggestions.len());
        for w in &suggestions {
            print_warning(w);
        }
    }
    Ok(())
}

fn cmd_graph(workdir: &Path, git_dir: &Path, config: &Config, file: &str) -> Result<()> {
    let (index, graph, _all_files) = build_index_and_graph(workdir, git_dir, config)?;
    cache::save_index(git_dir, &index)?;

    let node = graph.get_node(file);
    let deps = graph.dependencies(file);
    let dependents = graph.dependents(file);

    println!("{}", file.bold());

    if let Some(node) = node {
        if !node.related.is_empty() {
            println!("\n  {} (Related:)", "→ links to".dimmed());
            for r in &node.related {
                println!("    {}", r.cyan());
            }
        }
        if !node.see.is_empty() {
            println!("\n  {} (See:)", "→ sees".dimmed());
            for s in &node.see {
                println!("    {}", s.cyan());
            }
        }
    } else {
        println!("  (no @agents header found)");
    }

    if !dependents.is_empty() {
        println!("\n  {} (other files point here)", "← linked by".dimmed());
        for d in &dependents {
            println!("    {}", d.cyan());
        }
    }

    if deps.is_empty() && dependents.is_empty() {
        println!("  {} (no incoming or outgoing links)", "orphan".yellow());
    }

    Ok(())
}

fn cmd_reindex(workdir: &Path, git_dir: &Path, config: &Config) -> Result<()> {
    println!("Reindexing...");
    let (index, _graph, _all_files) = build_index_and_graph(workdir, git_dir, config)?;
    cache::save_index(git_dir, &index)?;
    let with = index.files_with_headers().len();
    let without = index.files_missing_headers().len();
    println!(
        "{} Index rebuilt: {} files with headers, {} without.",
        "✓".green(),
        with,
        without
    );
    Ok(())
}

fn cmd_build(workdir: &Path, _git_dir: &Path, config: &Config) -> Result<()> {
    let mut all_tags: Vec<AgentsTag> = Vec::new();

    for entry in WalkDir::new(workdir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let abs = entry.path();
        let rel = abs
            .strip_prefix(workdir)
            .unwrap_or(abs)
            .to_string_lossy()
            .to_string();

        if is_ignored(&rel, &config.ignore) {
            continue;
        }

        let content = match fs::read_to_string(abs) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let mut tags = parser::parse_all_agents_tags(&content, abs);
        if tags.is_empty() {
            continue;
        }

        for tag in &mut tags {
            tag.file = rel.clone();
        }
        all_tags.extend(tags);
    }

    all_tags.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));

    print!("{}", render_agent_context(&all_tags));

    Ok(())
}

/// Render all tags into the .agent-context Markdown format.
pub fn render_agent_context(tags: &[AgentsTag]) -> String {
    let mut out = String::from(
        "# agent-context\n<!-- Generated by git-agent-tags. Do not edit manually. -->",
    );

    for tag in tags {
        out.push_str("\n\n");
        let name_suffix = tag.name.as_deref().map_or(String::new(), |n| format!("#{}", n));
        let heading = match tag.kind {
            TagKind::FileHeader => format!("## {}{}", tag.file, name_suffix),
            TagKind::Inline => format!("## {}:{}{}", tag.file, tag.line, name_suffix),
        };
        out.push_str(&heading);
        let body = tag.text.join("\n");
        if !body.trim().is_empty() {
            out.push('\n');
            out.push_str(&body);
        }
    }

    out.push('\n');
    out
}

fn cmd_hook(
    workdir: &Path,
    git_dir: &Path,
    config: &Config,
    repo: &GitRepo,
    deep: bool,
) -> Result<()> {
    let (mut index, graph, all_files) = build_index_and_graph(workdir, git_dir, config)?;

    if deep {
        populate_header_commits(&mut index, repo);
    }

    cache::save_index(git_dir, &index)?;

    let mut errors: Vec<Warning> = Vec::new();
    let mut warnings: Vec<Warning> = Vec::new();

    // Broken refs are errors (block commit)
    for (src, dep) in graph.broken_refs(&all_files) {
        errors.push(Warning {
            file: src,
            level: WarnLevel::Broken,
            message: format!("Related: {} (file not found)", dep),
        });
    }

    let rename_warnings = check::check_renames(&graph, repo)?;
    for w in rename_warnings {
        match w.level {
            WarnLevel::Broken => errors.push(w),
            _ => warnings.push(w),
        }
    }

    // Staleness checks are warnings (print but don't block)
    for cached in index.files_with_headers() {
        if let Some(header) = &cached.header {
            let stale = check::check_git_staleness(
                &cached.path,
                header.start_line,
                header.end_line,
                header.lines_owned,
                repo,
                config.stale_commit_gap,
                config.stale_diff_percent,
            )?;
            warnings.extend(stale);

            if deep {
                if let Some(sha) = &header.last_header_commit {
                    let w =
                        check::check_regex_staleness(&cached.path, sha, &header.related, repo)?;
                    warnings.extend(w);
                }
            }
        }
    }

    if !errors.is_empty() {
        println!(
            "\n{} {} error(s) — commit blocked:\n",
            "✗".red().bold(),
            errors.len()
        );
        for w in &errors {
            print_warning(w);
        }
    }

    if !warnings.is_empty() {
        println!(
            "\n{} {} warning(s):\n",
            "⚠".yellow().bold(),
            warnings.len()
        );
        for w in &warnings {
            print_warning(w);
        }
    }

    if errors.is_empty() && warnings.is_empty() {
        println!("{}", "✓ agent-tags: no issues.".green());
    }

    if !errors.is_empty() {
        std::process::exit(1);
    }

    Ok(())
}

fn cmd_hook_install(git_dir: &Path) -> Result<()> {
    let hooks_dir = git_dir.join("hooks");
    fs::create_dir_all(&hooks_dir)?;

    let hook_path = hooks_dir.join("pre-commit");
    let hook_script = r#"#!/bin/sh
# git-agent-tags pre-commit hook
# Fails on broken references, warns on stale headers.

if command -v git-agent-tags >/dev/null 2>&1; then
    git-agent-tags hook
else
    echo "warning: git-agent-tags not installed, skipping check"
fi
"#;

    if hook_path.exists() {
        let existing = fs::read_to_string(&hook_path)?;
        if existing.contains("git-agent-tags") {
            println!("{} pre-commit hook already installed.", "✓".green());
            return Ok(());
        }
        // Append to existing hook
        let mut content = existing;
        if !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str("\n# git-agent-tags pre-commit hook\nif command -v git-agent-tags >/dev/null 2>&1; then\n    git-agent-tags hook\nfi\n");
        fs::write(&hook_path, content)?;
        println!(
            "{} Appended agent-tags check to existing pre-commit hook.",
            "✓".green()
        );
    } else {
        fs::write(&hook_path, hook_script)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&hook_path, fs::Permissions::from_mode(0o755))?;
        }
        println!("{} Installed pre-commit hook.", "✓".green());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tag(file: &str, line: usize, text: &[&str], kind: TagKind) -> AgentsTag {
        AgentsTag {
            file: file.to_string(),
            name: None,
            lines_owned: None,
            line,
            text: text.iter().map(|s| s.to_string()).collect(),
            kind,
        }
    }

    fn make_named_tag(file: &str, name: &str, line: usize, text: &[&str], kind: TagKind) -> AgentsTag {
        AgentsTag {
            file: file.to_string(),
            name: Some(name.to_string()),
            lines_owned: None,
            line,
            text: text.iter().map(|s| s.to_string()).collect(),
            kind,
        }
    }

    #[test]
    fn test_render_empty() {
        let out = render_agent_context(&[]);
        assert!(out.starts_with("# agent-context"));
        assert!(out.contains("Do not edit manually"));
    }

    #[test]
    fn test_render_file_header_no_line_number() {
        let tags = vec![make_tag("src/auth.ts", 1, &["Auth module."], TagKind::FileHeader)];
        let out = render_agent_context(&tags);
        assert!(out.contains("## src/auth.ts\n"));
        assert!(!out.contains("## src/auth.ts:"));
    }

    #[test]
    fn test_render_inline_has_line_number() {
        let tags = vec![make_tag("src/auth.ts", 42, &["Note about line 42."], TagKind::Inline)];
        let out = render_agent_context(&tags);
        assert!(out.contains("## src/auth.ts:42\n"));
    }

    #[test]
    fn test_render_sorted_by_file_then_line() {
        let tags = vec![
            make_tag("src/z.ts", 10, &["Z file."], TagKind::Inline),
            make_tag("src/a.ts", 5, &["A file."], TagKind::FileHeader),
            make_tag("src/z.ts", 3, &["Z earlier."], TagKind::Inline),
        ];
        // Sort as cmd_build would.
        let mut sorted = tags.clone();
        sorted.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
        let out = render_agent_context(&sorted);
        let pos_a = out.find("## src/a.ts").unwrap();
        let pos_z3 = out.find("## src/z.ts:3").unwrap();
        let pos_z10 = out.find("## src/z.ts:10").unwrap();
        assert!(pos_a < pos_z3);
        assert!(pos_z3 < pos_z10);
    }

    #[test]
    fn test_render_multiline_text() {
        let tags = vec![make_tag(
            "src/foo.ts",
            7,
            &["Line one.", "Line two."],
            TagKind::Inline,
        )];
        let out = render_agent_context(&tags);
        assert!(out.contains("Line one.\nLine two."));
    }

    #[test]
    fn test_render_named_file_header() {
        let tags = vec![make_named_tag("src/auth.ts", "auth-module", 1, &["Auth module."], TagKind::FileHeader)];
        let out = render_agent_context(&tags);
        assert!(out.contains("## src/auth.ts#auth-module\n"));
    }

    #[test]
    fn test_render_named_inline() {
        let tags = vec![make_named_tag("src/auth.ts", "token-check", 42, &["Check tokens."], TagKind::Inline)];
        let out = render_agent_context(&tags);
        assert!(out.contains("## src/auth.ts:42#token-check\n"));
    }

    #[test]
    fn test_populate_header_commits_sets_sha() {
        use std::process::Command;

        // Create a temp git repo with a file containing an @agents header
        let tmp = std::env::temp_dir().join(format!("agent-tags-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        Command::new("git").args(["init"]).current_dir(&tmp).output().unwrap();
        Command::new("git").args(["config", "user.email", "test@test.com"]).current_dir(&tmp).output().unwrap();
        Command::new("git").args(["config", "user.name", "Test"]).current_dir(&tmp).output().unwrap();

        // Write a file with an @agents header
        let file_content = "// @agents\n// Test file for staleness.\n// Related: other.ts\n\nfn main() {}\n";
        std::fs::write(tmp.join("test.rs"), file_content).unwrap();

        Command::new("git").args(["add", "."]).current_dir(&tmp).output().unwrap();
        Command::new("git").args(["commit", "-m", "initial"]).current_dir(&tmp).output().unwrap();

        // Build an index with the file
        let block = parser::parse_agents_block(file_content, std::path::Path::new("test.rs")).unwrap();
        let cached_header = cache::cached_header_from_block(&block);
        assert!(cached_header.last_header_commit.is_none(), "starts as None");

        let mut index = Index::new();
        index.upsert(CachedFile {
            path: "test.rs".to_string(),
            has_header: true,
            header: Some(cached_header),
            mtime_secs: None,
            file_size: None,
            tag_names: vec![],
        });

        // Open repo and populate
        let repo = GitRepo::open(&tmp).unwrap();
        populate_header_commits(&mut index, &repo);

        let header = index.get("test.rs").unwrap().header.as_ref().unwrap();
        assert!(
            header.last_header_commit.is_some(),
            "last_header_commit should be Some after populate_header_commits, got None"
        );

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_deep_check_detects_new_export() {
        use std::process::Command;

        let tmp = std::env::temp_dir().join(format!("agent-tags-deep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        Command::new("git").args(["init"]).current_dir(&tmp).output().unwrap();
        Command::new("git").args(["config", "user.email", "test@test.com"]).current_dir(&tmp).output().unwrap();
        Command::new("git").args(["config", "user.name", "Test"]).current_dir(&tmp).output().unwrap();

        // Commit 1: file with header
        let v1 = "// @agents\n// Auth module.\n\npub fn login() {}\n";
        std::fs::write(tmp.join("auth.rs"), v1).unwrap();
        Command::new("git").args(["add", "."]).current_dir(&tmp).output().unwrap();
        Command::new("git").args(["commit", "-m", "initial"]).current_dir(&tmp).output().unwrap();

        // Commit 2: add a new pub fn without updating the header
        let v2 = "// @agents\n// Auth module.\n\npub fn login() {}\n\npub fn logout() {}\n";
        std::fs::write(tmp.join("auth.rs"), v2).unwrap();
        Command::new("git").args(["add", "."]).current_dir(&tmp).output().unwrap();
        Command::new("git").args(["commit", "-m", "add logout"]).current_dir(&tmp).output().unwrap();

        // Parse and build index
        let block = parser::parse_agents_block(v2, std::path::Path::new("auth.rs")).unwrap();
        let cached_header = cache::cached_header_from_block(&block);
        let mut index = Index::new();
        index.upsert(CachedFile {
            path: "auth.rs".to_string(),
            has_header: true,
            header: Some(cached_header),
            mtime_secs: None,
            file_size: None,
            tag_names: vec![],
        });

        // Populate header commits
        let repo = GitRepo::open(&tmp).unwrap();
        populate_header_commits(&mut index, &repo);

        let header = index.get("auth.rs").unwrap().header.as_ref().unwrap();
        let sha = header.last_header_commit.as_ref().expect("should have SHA");

        // Now check_regex_staleness should detect the new pub fn
        let warnings = check::check_regex_staleness("auth.rs", sha, &header.related, &repo).unwrap();
        assert!(
            !warnings.is_empty(),
            "should detect new pub fn export, got no warnings"
        );
        assert!(
            warnings[0].message.contains("new exports"),
            "warning should mention new exports, got: {}",
            warnings[0].message
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
