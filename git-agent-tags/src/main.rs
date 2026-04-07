/**
 * @agents
 * CLI entry point. Wires up clap commands to the underlying modules.
 * Each subcommand delegates to parser, graph, git, check, cache, coverage, or sqlite.
 * Related: git-agent-tags/src/parser.rs, git-agent-tags/src/graph.rs, git-agent-tags/src/git.rs, git-agent-tags/src/check.rs, git-agent-tags/src/cache.rs, git-agent-tags/src/config.rs, git-agent-tags/src/coverage.rs, git-agent-tags/src/sqlite.rs
 */

mod cache;
mod check;
mod config;
mod coverage;
mod git;
mod graph;
mod parser;
mod sqlite;

use std::collections::{HashMap, HashSet};
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
use parser::{AgentsTag, RangeRole, TagKind};

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
    Context {
        /// Scope output to a specific file and its neighbors
        #[arg(long)]
        r#for: Option<String>,
        /// Number of hops in the reference graph (default: 1)
        #[arg(long, default_value = "1")]
        hops: usize,
    },
    /// Report @agents coverage metrics
    Coverage {
        /// Output as JSON for machine consumption
        #[arg(long)]
        json: bool,
    },
    /// Build a SQLite index for agent consumption
    Index {
        /// Delete and rebuild the database from scratch
        #[arg(long)]
        force: bool,
        /// Print database path and exit
        #[arg(long)]
        path: bool,
    },
    /// Query the SQLite tag index
    Query {
        #[command(subcommand)]
        subcmd: QueryCommand,
        /// Output as JSON
        #[arg(long, global = true)]
        json: bool,
    },
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

#[derive(Subcommand)]
enum QueryCommand {
    /// Full-text search across all tags
    Search { term: String },
    /// Show outgoing edges (dependencies) for a file
    Deps { file: String },
    /// Show incoming edges (reverse dependencies) for a file
    Rdeps { file: String },
    /// Show all tags for a specific file
    File { file: String },
    /// Show files with no tags, sorted by size descending
    Uncovered,
    /// Run arbitrary read-only SQL against tags.db
    Sql { query: String },
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
        Command::Coverage { json } => cmd_coverage(&workdir, &git_dir, &config, json),
        Command::Index { force, path } => cmd_index(&workdir, &git_dir, &config, force, path),
        Command::Query { subcmd, json } => cmd_query(&git_dir, subcmd, json),
        Command::Context { r#for: scope, hops } => cmd_build(&workdir, &git_dir, &config, scope.as_deref(), hops),
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

fn build_full_index(
    workdir: &Path,
    git_dir: &Path,
    config: &Config,
) -> Result<(Index, ReferenceGraph, HashSet<String>, HashMap<String, Vec<AgentsTag>>, HashMap<String, usize>)> {
    let prev_index = cache::load_index(git_dir).unwrap_or_default();
    let mut index = Index::new();
    let mut graph = ReferenceGraph::new();
    let mut all_files: HashSet<String> = HashSet::new();
    let mut all_tags_map: HashMap<String, Vec<AgentsTag>> = HashMap::new();
    let mut line_counts: HashMap<String, usize> = HashMap::new();

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
                // For cached files we still need inline tags — re-parse
                let content = match fs::read_to_string(abs) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                line_counts.insert(rel.clone(), content.lines().count());
                let mut tags = parser::parse_all_agents_tags(&content, abs);
                if !tags.is_empty() {
                    for tag in &mut tags {
                        tag.file = rel.clone();
                    }
                    all_tags_map.insert(rel, tags);
                }
                continue;
            }
        }

        let content = match fs::read_to_string(abs) {
            Ok(c) => c,
            Err(_) => continue, // binary or unreadable
        };
        line_counts.insert(rel.clone(), content.lines().count());

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

        // Collect full tags for SQLite
        if !all_tags.is_empty() {
            let mut tags_with_rel: Vec<AgentsTag> = all_tags;
            for tag in &mut tags_with_rel {
                tag.file = rel.clone();
            }
            all_tags_map.insert(rel.clone(), tags_with_rel);
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

    Ok((index, graph, all_files, all_tags_map, line_counts))
}

fn build_index_and_graph(
    workdir: &Path,
    git_dir: &Path,
    config: &Config,
) -> Result<(Index, ReferenceGraph, HashSet<String>)> {
    let (index, graph, all_files, _, _) = build_full_index(workdir, git_dir, config)?;
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

fn cmd_coverage(
    workdir: &Path,
    git_dir: &Path,
    config: &Config,
    json: bool,
) -> Result<()> {
    let (index, _graph, _all_files, all_tags, line_counts) =
        build_full_index(workdir, git_dir, config)?;

    let summary = coverage::compute_coverage(&index, &all_tags, &line_counts);

    if json {
        print_coverage_json(&summary);
    } else {
        print_coverage_text(&summary);
    }

    Ok(())
}

fn print_coverage_text(summary: &coverage::CoverageSummary) {
    let file_pct = if summary.total_files > 0 {
        summary.files_with_headers as f64 / summary.total_files as f64 * 100.0
    } else {
        0.0
    };
    let range_pct = if summary.total_lines > 0 {
        summary.range_lines as f64 / summary.total_lines as f64 * 100.0
    } else {
        0.0
    };

    println!("{}", "git-agent-tags coverage".bold());
    println!(
        "  File coverage:     {}/{} files ({:.1}%)",
        summary.files_with_headers, summary.total_files, file_pct
    );
    println!(
        "  Range coverage:    {}/{} lines ({:.1}%)",
        summary.range_lines, summary.total_lines, range_pct
    );

    if !summary.uncovered_hotspots.is_empty() {
        println!("\n  Top uncovered files:");
        for (path, lines) in summary.uncovered_hotspots.iter().take(10) {
            println!("    {:<50} {} lines", path, lines);
        }
    }

    if !summary.unmatched.is_empty() {
        println!("\n  Unmatched range tags:");
        for u in &summary.unmatched {
            let role_str = match u.role {
                RangeRole::Start => "start",
                RangeRole::End => "end",
            };
            let missing = if role_str == "start" { "end" } else { "start" };
            println!(
                "    {} {} — @agents({}, {}) at line {} has no matching {}",
                "⚠".yellow(),
                u.file.bold(),
                u.name,
                role_str,
                u.line,
                missing,
            );
        }
    }
}

fn print_coverage_json(summary: &coverage::CoverageSummary) {
    let file_pct = if summary.total_files > 0 {
        summary.files_with_headers as f64 / summary.total_files as f64 * 100.0
    } else {
        0.0
    };
    let range_pct = if summary.total_lines > 0 {
        summary.range_lines as f64 / summary.total_lines as f64 * 100.0
    } else {
        0.0
    };

    let obj = serde_json::json!({
        "file_coverage": {
            "with_headers": summary.files_with_headers,
            "total": summary.total_files,
            "percent": (file_pct * 10.0).round() / 10.0,
        },
        "range_coverage": {
            "covered_lines": summary.range_lines,
            "total_lines": summary.total_lines,
            "percent": (range_pct * 10.0).round() / 10.0,
        },
        "uncovered_hotspots": summary.uncovered_hotspots.iter().take(10).map(|(p, l)| {
            serde_json::json!({"path": p, "lines": l})
        }).collect::<Vec<_>>(),
        "unmatched_ranges": summary.unmatched.iter().map(|u| {
            serde_json::json!({
                "file": u.file,
                "name": u.name,
                "line": u.line,
                "role": match u.role { RangeRole::Start => "start", RangeRole::End => "end" },
            })
        }).collect::<Vec<_>>(),
    });
    println!("{}", serde_json::to_string_pretty(&obj).unwrap());
}

fn cmd_index(
    workdir: &Path,
    git_dir: &Path,
    config: &Config,
    force: bool,
    path_only: bool,
) -> Result<()> {
    let db = sqlite::db_path(git_dir);

    if path_only {
        println!("{}", db.display());
        return Ok(());
    }

    if force && db.exists() {
        fs::remove_file(&db)?;
    }

    let (index, graph, all_files, all_tags, line_counts) = build_full_index(workdir, git_dir, config)?;
    cache::save_index(git_dir, &index)?;

    let cov_summary = coverage::compute_coverage(&index, &all_tags, &line_counts);

    let mut conn = sqlite::open_or_create(&db)?;
    let stats = sqlite::write_index(&mut conn, &index, &graph, &all_tags, &all_files, Some(&cov_summary.per_file))?;

    let size = fs::metadata(&db).map(|m| m.len()).unwrap_or(0);
    let size_str = if size >= 1024 * 1024 {
        format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
    } else {
        format!("{} KB", size / 1024)
    };

    println!(
        "{} Index built: {} files, {} headers, {} inline tags, {} edges",
        "✓".green(),
        stats.files,
        stats.headers,
        stats.inline_tags,
        stats.edges,
    );
    println!("  Database: {} ({})", db.display(), size_str);
    Ok(())
}

fn cmd_query(git_dir: &Path, subcmd: QueryCommand, json: bool) -> Result<()> {
    let db = sqlite::db_path(git_dir);
    let conn = sqlite::open_readonly(&db)?;

    match subcmd {
        QueryCommand::Search { term } => query_search(&conn, &term, json),
        QueryCommand::Deps { file } => query_deps(&conn, &file, json),
        QueryCommand::Rdeps { file } => query_rdeps(&conn, &file, json),
        QueryCommand::File { file } => query_file(&conn, &file, json),
        QueryCommand::Uncovered => query_uncovered(&conn, json),
        QueryCommand::Sql { query } => query_sql(&conn, &query, json),
    }
}

fn query_search(conn: &rusqlite::Connection, term: &str, json: bool) -> Result<()> {
    let mut stmt = conn.prepare(
        "SELECT path, tag_type, name, body, warnings FROM tags_fts WHERE tags_fts MATCH ?1 ORDER BY rank LIMIT 50",
    )?;
    let rows: Vec<(String, String, Option<String>, String, Option<String>)> = stmt
        .query_map(rusqlite::params![term], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
        })?
        .filter_map(|r| r.ok())
        .collect();

    if rows.is_empty() {
        println!("No results.");
        return Ok(());
    }

    if json {
        let arr: Vec<serde_json::Value> = rows
            .iter()
            .map(|(path, tag_type, name, body, warnings)| {
                serde_json::json!({
                    "path": path,
                    "tag_type": tag_type,
                    "name": name,
                    "body": body,
                    "warnings": warnings,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
    } else {
        for (path, tag_type, name, body, _warnings) in &rows {
            let name_str = name.as_deref().unwrap_or("");
            println!(
                "  {}  {}  {}",
                path.bold(),
                tag_type.dimmed(),
                name_str.cyan()
            );
            for line in body.lines().take(3) {
                println!("    {}", line);
            }
            println!();
        }
    }
    Ok(())
}

fn query_deps(conn: &rusqlite::Connection, file: &str, json: bool) -> Result<()> {
    let mut stmt = conn.prepare(
        "SELECT e.target_path, e.edge_type FROM edges e JOIN files f ON e.source_id = f.id WHERE f.path = ?1 ORDER BY e.edge_type, e.target_path",
    )?;
    let rows: Vec<(String, String)> = stmt
        .query_map(rusqlite::params![file], |row| Ok((row.get(0)?, row.get(1)?)))?
        .filter_map(|r| r.ok())
        .collect();

    if rows.is_empty() {
        println!("No dependencies found for {}.", file);
        return Ok(());
    }

    if json {
        let arr: Vec<serde_json::Value> = rows
            .iter()
            .map(|(target, edge_type)| serde_json::json!({"target_path": target, "edge_type": edge_type}))
            .collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
    } else {
        println!("{} depends on:\n", file.bold());
        for (target, edge_type) in &rows {
            println!("  {}  {}", edge_type.dimmed(), target.cyan());
        }
    }
    Ok(())
}

fn query_rdeps(conn: &rusqlite::Connection, file: &str, json: bool) -> Result<()> {
    let mut stmt = conn.prepare(
        "SELECT f.path, e.edge_type FROM edges e JOIN files f ON e.source_id = f.id WHERE e.target_path = ?1 OR e.target_path LIKE ?2 ORDER BY f.path",
    )?;
    let pattern = format!("{}#%", file);
    let rows: Vec<(String, String)> = stmt
        .query_map(rusqlite::params![file, pattern], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?
        .filter_map(|r| r.ok())
        .collect();

    if rows.is_empty() {
        println!("No files depend on {}.", file);
        return Ok(());
    }

    if json {
        let arr: Vec<serde_json::Value> = rows
            .iter()
            .map(|(source, edge_type)| serde_json::json!({"source_path": source, "edge_type": edge_type}))
            .collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
    } else {
        println!("Files depending on {}:\n", file.bold());
        for (source, edge_type) in &rows {
            println!("  {}  ({})", source.cyan(), edge_type.dimmed());
        }
    }
    Ok(())
}

fn query_file(conn: &rusqlite::Connection, file: &str, json: bool) -> Result<()> {
    // Header
    let header: Option<(Option<String>, String, Option<String>, i64, i64)> = conn
        .query_row(
            "SELECT h.name, h.body, h.warnings, h.start_line, h.end_line FROM headers h JOIN files f ON h.file_id = f.id WHERE f.path = ?1",
            rusqlite::params![file],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .ok();

    // Inline tags
    let mut stmt = conn.prepare(
        "SELECT i.name, i.line, i.body, i.range_role FROM inline_tags i JOIN files f ON i.file_id = f.id WHERE f.path = ?1 ORDER BY i.line",
    )?;
    let inline_tags: Vec<(Option<String>, i64, String, Option<String>)> = stmt
        .query_map(rusqlite::params![file], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .filter_map(|r| r.ok())
        .collect();

    if header.is_none() && inline_tags.is_empty() {
        println!("No tags found for {}.", file);
        return Ok(());
    }

    if json {
        let header_json = header.as_ref().map(|(name, body, warnings, start, end)| {
            serde_json::json!({
                "name": name,
                "body": body,
                "warnings": warnings,
                "start_line": start,
                "end_line": end,
            })
        });
        let inline_json: Vec<serde_json::Value> = inline_tags
            .iter()
            .map(|(name, line, body, role)| {
                serde_json::json!({
                    "name": name,
                    "line": line,
                    "body": body,
                    "range_role": role,
                })
            })
            .collect();
        let obj = serde_json::json!({
            "path": file,
            "header": header_json,
            "inline_tags": inline_json,
        });
        println!("{}", serde_json::to_string_pretty(&obj)?);
    } else {
        println!("{}\n", file.bold());
        if let Some((name, body, warnings, start, end)) = &header {
            let name_str = name.as_deref().unwrap_or("(unnamed)");
            println!(
                "  {} (lines {}-{}):",
                "Header".bold(),
                start,
                end
            );
            println!("    name: {}", name_str.cyan());
            for line in body.lines() {
                println!("    {}", line);
            }
            if let Some(w) = warnings {
                for line in w.lines() {
                    println!("    {}", line.yellow());
                }
            }
            println!();
        }
        if !inline_tags.is_empty() {
            println!("  {}:", "Inline tags".bold());
            for (name, line, body, role) in &inline_tags {
                let name_str = name.as_deref().unwrap_or("");
                let role_str = role.as_deref().map(|r| format!(" ({})", r)).unwrap_or_default();
                println!(
                    "    L{}  {}{} {}",
                    line,
                    name_str.cyan(),
                    role_str.dimmed(),
                    body.lines().next().unwrap_or("")
                );
            }
        }
    }
    Ok(())
}

fn query_uncovered(conn: &rusqlite::Connection, json: bool) -> Result<()> {
    let mut stmt = conn.prepare(
        "SELECT f.path, COALESCE(c.total_lines, 0) as total_lines FROM files f LEFT JOIN coverage c ON c.file_id = f.id WHERE f.has_header = 0 ORDER BY COALESCE(c.total_lines, 0) DESC LIMIT 30",
    )?;
    let rows: Vec<(String, i64)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .filter_map(|r| r.ok())
        .collect();

    if rows.is_empty() {
        println!("{}", "All files have @agents tags.".green());
        return Ok(());
    }

    if json {
        let arr: Vec<serde_json::Value> = rows
            .iter()
            .map(|(path, lines)| serde_json::json!({"path": path, "total_lines": lines}))
            .collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
    } else {
        println!("Files with no @agents tags:\n");
        for (path, lines) in &rows {
            println!("  {:<50} {} lines", path, lines);
        }
    }
    Ok(())
}

fn query_sql(conn: &rusqlite::Connection, query: &str, json: bool) -> Result<()> {
    let mut stmt = conn.prepare(query)?;
    let col_count = stmt.column_count();
    let col_names: Vec<String> = (0..col_count)
        .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
        .collect();

    let mut rows: Vec<Vec<serde_json::Value>> = Vec::new();
    let mut raw_rows = stmt.query([])?;
    while let Some(row) = raw_rows.next()? {
        let mut vals = Vec::new();
        for i in 0..col_count {
            let val = match row.get_ref(i)? {
                rusqlite::types::ValueRef::Null => serde_json::Value::Null,
                rusqlite::types::ValueRef::Integer(n) => serde_json::json!(n),
                rusqlite::types::ValueRef::Real(f) => serde_json::json!(f),
                rusqlite::types::ValueRef::Text(s) => {
                    serde_json::Value::String(String::from_utf8_lossy(s).to_string())
                }
                rusqlite::types::ValueRef::Blob(_) => {
                    serde_json::Value::String("<blob>".to_string())
                }
            };
            vals.push(val);
        }
        rows.push(vals);
    }

    if rows.is_empty() {
        println!("No results.");
        return Ok(());
    }

    if json {
        let arr: Vec<serde_json::Value> = rows
            .iter()
            .map(|vals| {
                let mut obj = serde_json::Map::new();
                for (name, val) in col_names.iter().zip(vals.iter()) {
                    obj.insert(name.clone(), val.clone());
                }
                serde_json::Value::Object(obj)
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
    } else {
        // Compute column widths
        let mut widths: Vec<usize> = col_names.iter().map(|n| n.len()).collect();
        for vals in &rows {
            for (i, val) in vals.iter().enumerate() {
                let s = match val {
                    serde_json::Value::Null => "NULL".len(),
                    serde_json::Value::String(s) => s.len().min(60),
                    _ => format!("{}", val).len(),
                };
                widths[i] = widths[i].max(s).min(60);
            }
        }

        // Header
        let header: String = col_names
            .iter()
            .enumerate()
            .map(|(i, n)| format!("{:width$}", n, width = widths[i]))
            .collect::<Vec<_>>()
            .join("  ");
        println!("{}", header.bold());
        let sep: String = widths.iter().map(|w| "-".repeat(*w)).collect::<Vec<_>>().join("  ");
        println!("{}", sep.dimmed());

        // Rows
        for vals in &rows {
            let row_str: String = vals
                .iter()
                .enumerate()
                .map(|(i, val)| {
                    let s = match val {
                        serde_json::Value::Null => "NULL".to_string(),
                        serde_json::Value::String(s) => {
                            if s.len() > 60 {
                                format!("{}...", &s[..57])
                            } else {
                                s.clone()
                            }
                        }
                        _ => format!("{}", val),
                    };
                    format!("{:width$}", s, width = widths[i])
                })
                .collect::<Vec<_>>()
                .join("  ");
            println!("{}", row_str);
        }
    }
    Ok(())
}

fn cmd_build(workdir: &Path, git_dir: &Path, config: &Config, scope: Option<&str>, hops: usize) -> Result<()> {
    // When --for is given, build the graph and scope to the neighborhood
    let scoped_files: Option<HashSet<String>> = if let Some(file) = scope {
        let (_index, graph, _all_files) = build_index_and_graph(workdir, git_dir, config)?;
        let neighbors = graph.neighborhood(file, hops);
        Some(neighbors.into_iter().collect())
    } else {
        None
    };

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

        // Skip files outside the scoped neighborhood
        if let Some(ref allowed) = scoped_files {
            if !allowed.contains(&rel) {
                continue;
            }
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
            range_role: None,
            line,
            text: text.iter().map(|s| s.to_string()).collect(),
            kind,
        }
    }

    fn make_named_tag(file: &str, name: &str, line: usize, text: &[&str], kind: TagKind) -> AgentsTag {
        AgentsTag {
            file: file.to_string(),
            name: Some(name.to_string()),
            range_role: None,
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
