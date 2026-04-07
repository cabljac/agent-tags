/**
 * @agents
 * SQLite index builder for agent consumption.
 * Writes .git/agent-tags/tags.db with FTS5 search over all tag data.
 * Related: git-agent-tags/src/cache.rs, git-agent-tags/src/graph.rs, git-agent-tags/src/parser.rs
 */

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OpenFlags};

use crate::cache::Index;
use crate::coverage::FileCoverage;
use crate::graph::ReferenceGraph;
use crate::parser::{AgentsTag, RangeRole, TagKind};

const SCHEMA_VERSION: &str = "2";

const CREATE_TABLES: &str = "
CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS files (
    id         INTEGER PRIMARY KEY,
    path       TEXT NOT NULL UNIQUE,
    has_header INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_files_path ON files(path);

CREATE TABLE IF NOT EXISTS headers (
    id         INTEGER PRIMARY KEY,
    file_id    INTEGER NOT NULL UNIQUE REFERENCES files(id),
    name       TEXT,
    body       TEXT NOT NULL,
    warnings   TEXT,
    start_line INTEGER NOT NULL,
    end_line   INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_headers_name ON headers(name);

CREATE TABLE IF NOT EXISTS inline_tags (
    id         INTEGER PRIMARY KEY,
    file_id    INTEGER NOT NULL REFERENCES files(id),
    name       TEXT,
    line       INTEGER NOT NULL,
    body       TEXT NOT NULL,
    range_role TEXT
);
CREATE INDEX IF NOT EXISTS idx_inline_tags_file_id ON inline_tags(file_id);
CREATE INDEX IF NOT EXISTS idx_inline_tags_name ON inline_tags(name);

CREATE TABLE IF NOT EXISTS edges (
    id          INTEGER PRIMARY KEY,
    source_id   INTEGER NOT NULL REFERENCES files(id),
    target_path TEXT NOT NULL,
    edge_type   TEXT NOT NULL,
    target_id   INTEGER REFERENCES files(id)
);
CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source_id);
CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target_id);
CREATE INDEX IF NOT EXISTS idx_edges_target_path ON edges(target_path);

CREATE TABLE IF NOT EXISTS coverage (
    id           INTEGER PRIMARY KEY,
    file_id      INTEGER NOT NULL REFERENCES files(id),
    total_lines  INTEGER NOT NULL,
    header_lines INTEGER NOT NULL DEFAULT 0,
    range_lines  INTEGER NOT NULL DEFAULT 0,
    inline_lines INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_coverage_file_id ON coverage(file_id);
";

const CREATE_FTS: &str = "
CREATE VIRTUAL TABLE IF NOT EXISTS tags_fts USING fts5(
    path, tag_type, name, body, warnings,
    tokenize='porter unicode61'
);
";

pub struct Stats {
    pub files: usize,
    pub headers: usize,
    pub inline_tags: usize,
    pub edges: usize,
}

pub fn db_path(git_dir: &Path) -> PathBuf {
    git_dir.join("agent-tags").join("tags.db")
}

pub fn open_readonly(path: &Path) -> Result<Connection> {
    if !path.exists() {
        anyhow::bail!(
            "tags.db not found at {}. Run `git agent-tags index` first.",
            path.display()
        );
    }
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("Failed to open SQLite DB at {}", path.display()))?;
    Ok(conn)
}

pub fn open_or_create(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }
    let conn = Connection::open(path)
        .with_context(|| format!("Failed to open SQLite DB at {}", path.display()))?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
    Ok(conn)
}

fn drop_all(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP TABLE IF EXISTS coverage;
         DROP TABLE IF EXISTS edges;
         DROP TABLE IF EXISTS inline_tags;
         DROP TABLE IF EXISTS headers;
         DROP TABLE IF EXISTS files;
         DROP TABLE IF EXISTS meta;
         DROP TABLE IF EXISTS tags_fts;",
    )?;
    Ok(())
}

/// Build the full SQLite index in a single transaction.
pub fn write_index(
    conn: &mut Connection,
    index: &Index,
    graph: &ReferenceGraph,
    all_tags: &HashMap<String, Vec<AgentsTag>>,
    all_files: &HashSet<String>,
    coverage: Option<&[FileCoverage]>,
) -> Result<Stats> {
    drop_all(conn)?;
    conn.execute_batch(CREATE_TABLES)?;
    conn.execute_batch(CREATE_FTS)?;

    let tx = conn.transaction()?;
    let mut stats = Stats {
        files: 0,
        headers: 0,
        inline_tags: 0,
        edges: 0,
    };

    // 1. Insert files
    let file_ids: HashMap<String, i64> = {
        let mut map = HashMap::new();
        let mut stmt = tx.prepare("INSERT INTO files (path, has_header) VALUES (?1, ?2)")?;
        for cached in index.files.values() {
            stmt.execute(params![cached.path, cached.has_header as i32])?;
            let id = tx.last_insert_rowid();
            map.insert(cached.path.clone(), id);
            stats.files += 1;
        }
        // Insert files that are in all_files but not in the index (rare edge case)
        for path in all_files {
            if !map.contains_key(path) {
                stmt.execute(params![path, 0])?;
                let id = tx.last_insert_rowid();
                map.insert(path.clone(), id);
                stats.files += 1;
            }
        }
        map
    };

    // 2. Insert headers
    {
        let mut stmt = tx.prepare(
            "INSERT INTO headers (file_id, name, body, warnings, start_line, end_line) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        let mut fts_stmt = tx.prepare(
            "INSERT INTO tags_fts (path, tag_type, name, body, warnings) VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for cached in index.files.values() {
            if let Some(header) = &cached.header {
                if let Some(&file_id) = file_ids.get(&cached.path) {
                    let body = header.body.join("\n");
                    let warnings = if header.warnings.is_empty() {
                        None
                    } else {
                        Some(header.warnings.join("\n"))
                    };
                    stmt.execute(params![
                        file_id,
                        header.name,
                        body,
                        warnings,
                        header.start_line,
                        header.end_line,
                    ])?;
                    fts_stmt.execute(params![
                        cached.path,
                        "header",
                        header.name,
                        body,
                        warnings,
                    ])?;
                    stats.headers += 1;
                }
            }
        }
    }

    // 3. Insert inline tags
    {
        let mut stmt = tx.prepare(
            "INSERT INTO inline_tags (file_id, name, line, body, range_role) VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        let mut fts_stmt = tx.prepare(
            "INSERT INTO tags_fts (path, tag_type, name, body, warnings) VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for (path, tags) in all_tags {
            if let Some(&file_id) = file_ids.get(path) {
                for tag in tags {
                    if tag.kind == TagKind::Inline {
                        let body = tag.text.join("\n");
                        let range_role = tag.range_role.as_ref().map(|r| match r {
                            RangeRole::Start => "start",
                            RangeRole::End => "end",
                        });
                        stmt.execute(params![file_id, tag.name, tag.line, body, range_role])?;
                        fts_stmt.execute(params![path, "inline", tag.name, body, Option::<String>::None])?;
                        stats.inline_tags += 1;
                    }
                }
            }
        }
    }

    // 4. Insert edges
    {
        let mut stmt = tx.prepare(
            "INSERT INTO edges (source_id, target_path, edge_type, target_id) VALUES (?1, ?2, ?3, ?4)",
        )?;
        for file in graph.all_files() {
            if let Some(node) = graph.get_node(file) {
                if let Some(&source_id) = file_ids.get(file) {
                    for related in &node.related {
                        if related.starts_with("http://") || related.starts_with("https://") {
                            continue;
                        }
                        let base = related.split_once('#').map_or(related.as_str(), |(b, _)| b);
                        let target_id = file_ids.get(base).copied();
                        stmt.execute(params![source_id, related, "related", target_id])?;
                        stats.edges += 1;
                    }
                    for see in &node.see {
                        if see.starts_with("http://") || see.starts_with("https://") {
                            continue;
                        }
                        let base = see.split_once('#').map_or(see.as_str(), |(b, _)| b);
                        let target_id = file_ids.get(base).copied();
                        stmt.execute(params![source_id, see, "see", target_id])?;
                        stats.edges += 1;
                    }
                }
            }
        }
    }

    // 5. Insert coverage
    if let Some(file_coverages) = coverage {
        let mut cov_stmt = tx.prepare(
            "INSERT INTO coverage (file_id, total_lines, header_lines, range_lines, inline_lines) VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        let mut total_lines_sum: i64 = 0;
        let mut range_lines_sum: i64 = 0;
        for fc in file_coverages {
            if let Some(&fid) = file_ids.get(&fc.path) {
                cov_stmt.execute(params![
                    fid,
                    fc.total_lines,
                    fc.header_lines,
                    fc.range_lines,
                    fc.inline_lines,
                ])?;
                total_lines_sum += fc.total_lines as i64;
                range_lines_sum += fc.range_lines as i64;
            }
        }
        let files_with_headers = file_coverages.iter().filter(|fc| fc.header_lines > 0).count();
        let total_files = file_coverages.len();
        let pct = if total_files > 0 {
            files_with_headers as f64 / total_files as f64 * 100.0
        } else {
            0.0
        };
        tx.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)",
            params!["coverage_file_percent", format!("{:.1}", pct)],
        )?;
        tx.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)",
            params!["coverage_range_lines", range_lines_sum.to_string()],
        )?;
        tx.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)",
            params!["coverage_total_lines", total_lines_sum.to_string()],
        )?;
    }

    // 6. Insert meta
    {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs().to_string())
            .unwrap_or_default();
        tx.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)",
            params!["schema_version", SCHEMA_VERSION],
        )?;
        tx.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)",
            params!["built_at", now],
        )?;
    }

    tx.commit()?;
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{CachedFile, CachedHeader};
    use crate::graph::GraphNode;

    fn make_test_data() -> (Index, ReferenceGraph, HashMap<String, Vec<AgentsTag>>, HashSet<String>) {
        let mut index = Index::new();
        index.upsert(CachedFile {
            path: "src/auth.rs".to_string(),
            has_header: true,
            header: Some(CachedHeader {
                name: Some("auth-module".to_string()),
                body: vec!["Handles authentication and token validation.".to_string()],
                related: vec!["src/user.rs".to_string()],
                see: vec![],
                warnings: vec!["Don't bypass token checks.".to_string()],
                start_line: 1,
                end_line: 5,
                last_header_commit: None,
            }),
            mtime_secs: None,
            file_size: None,
            tag_names: vec!["auth-module".to_string()],
        });
        index.upsert(CachedFile {
            path: "src/user.rs".to_string(),
            has_header: true,
            header: Some(CachedHeader {
                name: None,
                body: vec!["User model and queries.".to_string()],
                related: vec![],
                see: vec![],
                warnings: vec![],
                start_line: 1,
                end_line: 3,
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

        let mut graph = ReferenceGraph::new();
        graph.add_node(GraphNode {
            file: "src/auth.rs".to_string(),
            related: vec!["src/user.rs".to_string()],
            see: vec![],
        });
        graph.add_node(GraphNode {
            file: "src/user.rs".to_string(),
            related: vec![],
            see: vec![],
        });

        let mut all_tags: HashMap<String, Vec<AgentsTag>> = HashMap::new();
        all_tags.insert(
            "src/auth.rs".to_string(),
            vec![
                AgentsTag {
                    file: "src/auth.rs".to_string(),
                    name: Some("auth-module".to_string()),
                    range_role: None,
                    line: 1,
                    text: vec!["Handles authentication and token validation.".to_string()],
                    kind: TagKind::FileHeader,
                },
                AgentsTag {
                    file: "src/auth.rs".to_string(),
                    name: Some("token-check".to_string()),
                    range_role: Some(RangeRole::Start),
                    line: 20,
                    text: vec!["Validates JWT tokens.".to_string()],
                    kind: TagKind::Inline,
                },
            ],
        );

        let all_files: HashSet<String> = ["src/auth.rs", "src/user.rs", "src/main.rs"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        (index, graph, all_tags, all_files)
    }

    #[test]
    fn test_schema_creation() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CREATE_TABLES).unwrap();
        conn.execute_batch(CREATE_FTS).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_write_and_query() {
        let (index, graph, all_tags, all_files) = make_test_data();
        let mut conn = Connection::open_in_memory().unwrap();

        let stats = write_index(&mut conn, &index, &graph, &all_tags, &all_files, None).unwrap();
        assert_eq!(stats.files, 3);
        assert_eq!(stats.headers, 2);
        assert_eq!(stats.inline_tags, 1);
        assert_eq!(stats.edges, 1);
    }

    #[test]
    fn test_fts_search() {
        let (index, graph, all_tags, all_files) = make_test_data();
        let mut conn = Connection::open_in_memory().unwrap();
        write_index(&mut conn, &index, &graph, &all_tags, &all_files, None).unwrap();

        let mut stmt = conn
            .prepare("SELECT path, name, body FROM tags_fts WHERE tags_fts MATCH 'authentication' ORDER BY rank")
            .unwrap();
        let results: Vec<(String, Option<String>, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert!(!results.is_empty());
        assert!(results.iter().any(|(path, _, _)| path == "src/auth.rs"));
    }

    #[test]
    fn test_edge_resolution() {
        let (index, graph, all_tags, all_files) = make_test_data();
        let mut conn = Connection::open_in_memory().unwrap();
        write_index(&mut conn, &index, &graph, &all_tags, &all_files, None).unwrap();

        // Check that the edge from auth.rs -> user.rs has a resolved target_id
        let target_id: Option<i64> = conn
            .query_row(
                "SELECT e.target_id FROM edges e JOIN files f ON e.source_id = f.id WHERE f.path = 'src/auth.rs'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(target_id.is_some());
    }

    #[test]
    fn test_coverage_query() {
        let (index, graph, all_tags, all_files) = make_test_data();
        let mut conn = Connection::open_in_memory().unwrap();
        write_index(&mut conn, &index, &graph, &all_tags, &all_files, None).unwrap();

        let (total, documented): (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), SUM(has_header) FROM files",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(total, 3);
        assert_eq!(documented, 2);
    }

    #[test]
    fn test_warnings_query() {
        let (index, graph, all_tags, all_files) = make_test_data();
        let mut conn = Connection::open_in_memory().unwrap();
        write_index(&mut conn, &index, &graph, &all_tags, &all_files, None).unwrap();

        let mut stmt = conn
            .prepare("SELECT f.path, h.warnings FROM headers h JOIN files f ON h.file_id = f.id WHERE h.warnings IS NOT NULL AND h.warnings != ''")
            .unwrap();
        let results: Vec<(String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "src/auth.rs");
        assert!(results[0].1.contains("Don't bypass"));
    }

    #[test]
    fn test_rebuild_is_idempotent() {
        let (index, graph, all_tags, all_files) = make_test_data();
        let mut conn = Connection::open_in_memory().unwrap();

        let stats1 = write_index(&mut conn, &index, &graph, &all_tags, &all_files, None).unwrap();
        let stats2 = write_index(&mut conn, &index, &graph, &all_tags, &all_files, None).unwrap();
        assert_eq!(stats1.files, stats2.files);
        assert_eq!(stats1.headers, stats2.headers);

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn test_coverage_table() {
        let (index, graph, all_tags, all_files) = make_test_data();
        let mut conn = Connection::open_in_memory().unwrap();

        let coverage = vec![
            FileCoverage {
                path: "src/auth.rs".to_string(),
                total_lines: 50,
                header_lines: 5,
                range_lines: 11,
                inline_lines: 1,
            },
            FileCoverage {
                path: "src/user.rs".to_string(),
                total_lines: 30,
                header_lines: 3,
                range_lines: 0,
                inline_lines: 0,
            },
            FileCoverage {
                path: "src/main.rs".to_string(),
                total_lines: 100,
                header_lines: 0,
                range_lines: 0,
                inline_lines: 0,
            },
        ];

        write_index(&mut conn, &index, &graph, &all_tags, &all_files, Some(&coverage)).unwrap();

        let (total, range): (i64, i64) = conn
            .query_row(
                "SELECT SUM(total_lines), SUM(range_lines) FROM coverage",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(total, 180);
        assert_eq!(range, 11);

        let pct: String = conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'coverage_file_percent'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(pct, "66.7");
    }
}
