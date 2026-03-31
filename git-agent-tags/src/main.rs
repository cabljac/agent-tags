/**
 * @agents
 * CLI entry point. Wires up clap commands to the underlying modules.
 * Each subcommand delegates to parser, graph, git, check, or cache.
 * Related: git-agent-headers/src/parser.rs, git-agent-headers/src/graph.rs, git-agent-headers/src/git.rs, git-agent-headers/src/check.rs, git-agent-headers/src/cache.rs, git-agent-headers/src/config.rs
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
    name = "git-agent-headers",
    about = "Manage per-file @agents context headers in codebases",
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
        Command::Context => cmd_build(&workdir, &config),
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn build_index_and_graph(
    workdir: &Path,
    config: &Config,
) -> Result<(Index, ReferenceGraph, HashSet<String>)> {
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

        let content = match fs::read_to_string(abs) {
            Ok(c) => c,
            Err(_) => continue, // binary or unreadable
        };

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
            });
        } else {
            index.upsert(CachedFile {
                path: rel,
                has_header: false,
                header: None,
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
    let (index, graph, all_files) = build_index_and_graph(workdir, config)?;

    let total = all_files.len();
    let with_headers = index.files_with_headers().len();
    let missing = index.files_missing_headers().len();
    let broken = graph.broken_refs(&all_files);

    println!("{}", "git-agent-headers status".bold());
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
    let (index, graph, all_files) = build_index_and_graph(workdir, config)?;
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
    let (index, graph, all_files) = build_index_and_graph(workdir, config)?;
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
    let (index, _graph, _all_files) = build_index_and_graph(workdir, config)?;
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
    let (index, graph, _all_files) = build_index_and_graph(workdir, config)?;
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
    let (index, graph, _all_files) = build_index_and_graph(workdir, config)?;
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
    let (index, _graph, _all_files) = build_index_and_graph(workdir, config)?;
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

fn cmd_build(workdir: &Path, config: &Config) -> Result<()> {
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
        "# agent-context\n<!-- Generated by git-agent-headers. Do not edit manually. -->",
    );

    for tag in tags {
        out.push_str("\n\n");
        let heading = match tag.kind {
            TagKind::FileHeader => format!("## {}", tag.file),
            TagKind::Inline => format!("## {}:{}", tag.file, tag.line),
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


#[cfg(test)]
mod tests {
    use super::*;

    fn make_tag(file: &str, line: usize, text: &[&str], kind: TagKind) -> AgentsTag {
        AgentsTag {
            file: file.to_string(),
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
}
