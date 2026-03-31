/**
 * @agents
 * Index cache stored in .git/agent-headers/index.json.
 * Derived from parsing all files; rebuilt on demand, not committed.
 * Related: git-agent-headers/src/parser.rs, git-agent-headers/src/graph.rs
 */

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::parser::AgentsBlock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedFile {
    pub path: String,
    pub has_header: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<CachedHeader>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedHeader {
    pub body: Vec<String>,
    pub related: Vec<String>,
    pub see: Vec<String>,
    pub warnings: Vec<String>,
    pub start_line: usize,
    pub end_line: usize,
    /// Git commit hash when this header was last seen to change
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_header_commit: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Index {
    pub version: u32,
    pub files: HashMap<String, CachedFile>,
}

impl Index {
    pub fn new() -> Self {
        Self {
            version: 1,
            files: HashMap::new(),
        }
    }

    pub fn upsert(&mut self, file: CachedFile) {
        self.files.insert(file.path.clone(), file);
    }

    #[allow(dead_code)]
    pub fn get(&self, path: &str) -> Option<&CachedFile> {
        self.files.get(path)
    }

    pub fn files_with_headers(&self) -> Vec<&CachedFile> {
        self.files.values().filter(|f| f.has_header).collect()
    }

    pub fn files_missing_headers(&self) -> Vec<&CachedFile> {
        self.files.values().filter(|f| !f.has_header).collect()
    }
}

/// Resolve the cache directory from the repo root.
pub fn cache_dir(git_dir: &Path) -> PathBuf {
    git_dir.join("agent-headers")
}

pub fn index_path(git_dir: &Path) -> PathBuf {
    cache_dir(git_dir).join("index.json")
}

#[allow(dead_code)]
pub fn load_index(git_dir: &Path) -> Result<Index> {
    let path = index_path(git_dir);
    if !path.exists() {
        return Ok(Index::new());
    }
    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read cache at {}", path.display()))?;
    serde_json::from_str(&content).with_context(|| "Failed to parse cache index")
}

pub fn save_index(git_dir: &Path, index: &Index) -> Result<()> {
    let dir = cache_dir(git_dir);
    fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create cache dir {}", dir.display()))?;
    let path = index_path(git_dir);
    let content = serde_json::to_string_pretty(index)?;
    fs::write(&path, content)
        .with_context(|| format!("Failed to write cache at {}", path.display()))?;
    Ok(())
}

pub fn cached_header_from_block(block: &AgentsBlock) -> CachedHeader {
    CachedHeader {
        body: block.body.clone(),
        related: block.related.clone(),
        see: block.see.clone(),
        warnings: block.warnings.clone(),
        start_line: block.start_line,
        end_line: block.end_line,
        last_header_commit: None,
    }
}
