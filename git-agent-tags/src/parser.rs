/**
 * @agents
 * Extracts @agents blocks from source files.
 * Handles all supported comment styles based on file extension.
 * Related: git-agent-headers/src/graph.rs, git-agent-headers/src/cache.rs
 */

use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct AgentsBlock {
    /// The raw header text (entire comment block)
    pub raw: String,
    /// Free-form description lines
    pub body: Vec<String>,
    /// File paths from Related: lines
    pub related: Vec<String>,
    /// Links/paths from See: lines
    pub see: Vec<String>,
    /// Warning lines (Don't, Warning:, Note:, Avoid:)
    pub warnings: Vec<String>,
    /// Line number where the block starts (1-indexed)
    pub start_line: usize,
    /// Line number where the block ends (1-indexed)
    pub end_line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum CommentStyle {
    /// /** */ or /* */ blocks, also // lines
    CStyle,
    /// # lines
    Hash,
    /// -- lines or {- -} blocks
    Lua,
    /// -- lines
    Haskell,
    /// """ docstrings or # lines
    Python,
}

fn comment_style(ext: &str) -> CommentStyle {
    match ext {
        "ts" | "tsx" | "js" | "jsx" | "java" | "kt" | "scala" | "go" | "rs" | "c" | "cpp"
        | "h" | "cs" | "swift" => CommentStyle::CStyle,
        "py" => CommentStyle::Python,
        "rb" | "sh" | "bash" | "yaml" | "yml" | "toml" | "ex" | "exs" => CommentStyle::Hash,
        "lua" => CommentStyle::Lua,
        "hs" => CommentStyle::Haskell,
        _ => CommentStyle::Hash,
    }
}

/// Discriminates how an @agents tag was discovered.
#[derive(Debug, Clone, PartialEq)]
pub enum TagKind {
    /// Block at the top of the file (`@agents` on its own line, first 30 lines).
    FileHeader,
    /// Inline annotation anywhere (`@agents:` with a colon).
    Inline,
}

/// A single @agents annotation, wherever it appears in the file.
/// `file` is left empty by the parser — the caller sets it to the repo-root-relative path.
#[derive(Debug, Clone)]
pub struct AgentsTag {
    pub file: String,
    /// 1-indexed line number where this tag starts.
    pub line: usize,
    /// Annotation text lines. For FileHeader, reconstructed from the AgentsBlock fields.
    /// For Inline, the text after `@agents:` plus any continuation lines.
    pub text: Vec<String>,
    pub kind: TagKind,
}

/// Returns the single-line comment prefix for a given style.
fn line_comment_prefix(style: CommentStyle) -> &'static str {
    match style {
        CommentStyle::CStyle => "//",
        CommentStyle::Hash | CommentStyle::Python => "#",
        CommentStyle::Lua | CommentStyle::Haskell => "--",
    }
}

/// Scan the entire file for all @agents tags (file-header blocks and inline annotations).
/// The `file` field on each returned tag is left empty — set it to the relative path after calling.
pub fn parse_all_agents_tags(content: &str, file_path: &Path) -> Vec<AgentsTag> {
    let ext = file_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let style = comment_style(ext);
    let prefix = line_comment_prefix(style);
    let lines: Vec<&str> = content.lines().collect();

    let mut tags: Vec<AgentsTag> = Vec::new();

    // Step 1: file-header block (reuse existing parser).
    let header_range = if let Some(block) = parse_agents_block(content, file_path) {
        let range = (block.start_line, block.end_line);
        tags.push(agents_block_to_tag(&block));
        Some(range)
    } else {
        None
    };

    // Step 2: inline scan — look for `@agents:` anywhere in the file.
    let mut i = 0;
    while i < lines.len() {
        let line_no = i + 1; // 1-indexed

        // Skip lines that belong to the file-header block.
        if let Some((start, end)) = header_range {
            if line_no >= start && line_no <= end {
                i += 1;
                continue;
            }
        }

        let trimmed = lines[i].trim();

        // Strip the comment prefix to get the content.
        let content_opt = strip_comment_prefix(trimmed, prefix, style);
        if let Some(comment_content) = content_opt {
            if let Some(rest) = comment_content.strip_prefix("@agents:") {
                let first_text = rest.trim().to_string();
                let tag_start = line_no;
                let mut text = vec![first_text];

                // Collect continuation lines.
                let mut j = i + 1;
                while j < lines.len() {
                    let next = lines[j].trim();
                    if next.is_empty() {
                        break;
                    }
                    match strip_comment_prefix(next, prefix, style) {
                        None => break,
                        Some(c) => {
                            // Stop if the next line is itself an @agents: tag.
                            if c.starts_with("@agents:") {
                                break;
                            }
                            text.push(c.to_string());
                            j += 1;
                        }
                    }
                }

                tags.push(AgentsTag {
                    file: String::new(),
                    line: tag_start,
                    text,
                    kind: TagKind::Inline,
                });

                i = j;
                continue;
            }
        }

        i += 1;
    }

    tags
}

/// Convert an AgentsBlock (file-header) into an AgentsTag.
fn agents_block_to_tag(block: &AgentsBlock) -> AgentsTag {
    let mut text: Vec<String> = block.body.clone();
    if !block.related.is_empty() {
        text.push(format!("Related: {}", block.related.join(", ")));
    }
    if !block.see.is_empty() {
        text.push(format!("See: {}", block.see.join(", ")));
    }
    text.extend(block.warnings.clone());

    AgentsTag {
        file: String::new(),
        line: block.start_line,
        text,
        kind: TagKind::FileHeader,
    }
}

/// Strip a comment prefix from a trimmed line, returning the inner content if it matched.
/// For CStyle, also handles block-comment inner lines (leading `*`).
fn strip_comment_prefix<'a>(
    trimmed: &'a str,
    prefix: &str,
    style: CommentStyle,
) -> Option<&'a str> {
    if let Some(rest) = trimmed.strip_prefix(prefix) {
        return Some(rest.trim());
    }
    // For C-style, also match block-comment inner lines: `* content` or `*/`
    if style == CommentStyle::CStyle {
        if let Some(rest) = trimmed.strip_prefix('*') {
            let inner = rest.trim();
            // Skip bare closing `*/`
            if inner == "/" || inner.is_empty() {
                return None;
            }
            return Some(inner);
        }
    }
    None
}

/// Returns true only if the comment line content is exactly the block-header marker `@agents`.
/// This distinguishes it from the inline form `@agents: text`.
fn is_block_marker(line: &str, comment_prefix: &str) -> bool {
    // Strip the comment prefix (// or # or --) and check for exact match.
    if let Some(rest) = line.strip_prefix(comment_prefix) {
        return rest.trim() == "@agents";
    }
    // Block comment inner lines start with `*`.
    if let Some(rest) = line.strip_prefix('*') {
        return rest.trim() == "@agents";
    }
    // Opening line of a block comment may include the marker inline: `/** @agents`.
    if line.starts_with("/**") || line.starts_with("/*") {
        let inner = line
            .trim_start_matches('/')
            .trim_start_matches('*')
            .trim_end_matches('/')
            .trim_end_matches('*')
            .trim();
        return inner == "@agents";
    }
    false
}

/// Parse an @agents block from source text.
/// Returns None if no valid block is found within the first 30 lines.
pub fn parse_agents_block(content: &str, file_path: &Path) -> Option<AgentsBlock> {
    let ext = file_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let style = comment_style(ext);

    let lines: Vec<&str> = content.lines().collect();
    let search_limit = lines.len().min(30);

    match style {
        CommentStyle::CStyle => parse_c_style(&lines, search_limit),
        CommentStyle::Hash | CommentStyle::Lua | CommentStyle::Haskell => {
            parse_line_comment(&lines, search_limit, style)
        }
        CommentStyle::Python => {
            // Try """ first, then #
            parse_python_docstring(&lines, search_limit)
                .or_else(|| parse_line_comment(&lines, search_limit, CommentStyle::Hash))
        }
    }
}

fn parse_c_style(lines: &[&str], limit: usize) -> Option<AgentsBlock> {
    let mut i = 0;
    while i < limit {
        let trimmed = lines[i].trim();

        // Try block comment: /** or /*
        if trimmed.starts_with("/**") || trimmed.starts_with("/*") {
            let start = i + 1;
            let mut block_lines: Vec<String> = Vec::new();
            let mut found_marker = false;

            // Check if @agents is on the opening line
            if is_block_marker(trimmed, "//") {
                found_marker = true;
            }
            block_lines.push(trimmed.to_string());

            let mut j = i + 1;
            while j < lines.len() {
                let bl = lines[j].trim();
                if is_block_marker(bl, "//") {
                    found_marker = true;
                }
                if bl.ends_with("*/") || bl == "*/" {
                    block_lines.push(bl.to_string());
                    if found_marker {
                        let raw = block_lines.join("\n");
                        let body_lines = extract_inner_lines_block(&block_lines);
                        return Some(build_block(raw, body_lines, start, j + 1));
                    }
                    break;
                }
                block_lines.push(bl.to_string());
                j += 1;
            }
            if found_marker && j >= lines.len() {
                // Unclosed block — treat up to end
                let raw = block_lines.join("\n");
                let body_lines = extract_inner_lines_block(&block_lines);
                return Some(build_block(raw, body_lines, start, j));
            }
        }

        // Try // comment lines
        if trimmed.starts_with("//") {
            let start = i + 1;
            let mut block_lines: Vec<String> = vec![trimmed.to_string()];
            let mut found_marker = is_block_marker(trimmed, "//");
            let mut j = i + 1;

            while j < lines.len() {
                let bl = lines[j].trim();
                if !bl.starts_with("//") {
                    break;
                }
                // Only accept @agents marker if still within limit
                if j < limit && is_block_marker(bl, "//") {
                    found_marker = true;
                }
                block_lines.push(bl.to_string());
                j += 1;
            }

            if found_marker {
                let raw = block_lines.join("\n");
                let body_lines = extract_inner_lines_line_comment(&block_lines, "//");
                return Some(build_block(raw, body_lines, start, j));
            }
        }

        i += 1;
    }
    None
}

fn parse_line_comment(lines: &[&str], limit: usize, style: CommentStyle) -> Option<AgentsBlock> {
    let prefix = match style {
        CommentStyle::Hash | CommentStyle::Python => "#",
        CommentStyle::Lua | CommentStyle::Haskell => "--",
        _ => "#",
    };

    let mut i = 0;
    while i < limit {
        let trimmed = lines[i].trim();
        if trimmed.starts_with(prefix) {
            let start = i + 1;
            let mut block_lines: Vec<String> = vec![trimmed.to_string()];
            let mut found_marker = is_block_marker(trimmed, prefix);
            let mut j = i + 1;

            while j < lines.len() {
                let bl = lines[j].trim();
                if !bl.starts_with(prefix) && !bl.is_empty() {
                    break;
                }
                if bl.starts_with(prefix) {
                    if j < limit && is_block_marker(bl, prefix) {
                        found_marker = true;
                    }
                    block_lines.push(bl.to_string());
                }
                j += 1;
            }

            if found_marker {
                let raw = block_lines.join("\n");
                let body_lines = extract_inner_lines_line_comment(&block_lines, prefix);
                return Some(build_block(raw, body_lines, start, j));
            }
        }
        i += 1;
    }
    None
}

fn parse_python_docstring(lines: &[&str], limit: usize) -> Option<AgentsBlock> {
    let mut i = 0;
    while i < limit {
        let trimmed = lines[i].trim();
        if trimmed.starts_with("\"\"\"") || trimmed.starts_with("'''") {
            let delim = if trimmed.starts_with("\"\"\"") {
                "\"\"\""
            } else {
                "'''"
            };
            let start = i + 1;
            let mut block_lines: Vec<String> = vec![trimmed.to_string()];
            let mut found_marker = trimmed.contains("@agents") && !trimmed.contains("@agents:");

            // Single-line docstring
            let after_open = &trimmed[3..];
            if after_open.contains(delim) {
                if found_marker {
                    let raw = trimmed.to_string();
                    let body = vec![after_open
                        .trim_end_matches(delim)
                        .trim()
                        .to_string()];
                    return Some(build_block(raw, body, start, i + 1));
                }
                i += 1;
                continue;
            }

            let mut j = i + 1;
            while j < lines.len() {
                let bl = lines[j].trim();
                if bl.contains("@agents") && !bl.contains("@agents:") {
                    found_marker = true;
                }
                block_lines.push(bl.to_string());
                if bl.ends_with(delim) || bl == delim {
                    if found_marker {
                        let raw = block_lines.join("\n");
                        let body_lines = extract_inner_lines_docstring(&block_lines, delim);
                        return Some(build_block(raw, body_lines, start, j + 1));
                    }
                    break;
                }
                j += 1;
            }
        }
        i += 1;
    }
    None
}

fn extract_inner_lines_block(lines: &[String]) -> Vec<String> {
    // Skip first (opening /**) and last (closing */) delimiter lines, process the rest.
    let inner = if lines.len() >= 2 {
        &lines[1..lines.len() - 1]
    } else {
        return vec![];
    };
    inner
        .iter()
        .map(|l| {
            let t = l.trim();
            // Strip leading * (common in /** */ blocks)
            let t = t.trim_start_matches('*').trim();
            t.to_string()
        })
        .collect()
}

fn extract_inner_lines_line_comment(lines: &[String], prefix: &str) -> Vec<String> {
    lines
        .iter()
        .map(|l| {
            let t = l.trim();
            let t = t.trim_start_matches(prefix).trim();
            t.to_string()
        })
        .collect()
}

fn extract_inner_lines_docstring(lines: &[String], delim: &str) -> Vec<String> {
    lines
        .iter()
        .map(|l| {
            l.trim()
                .trim_start_matches(delim)
                .trim_end_matches(delim)
                .trim()
                .to_string()
        })
        .filter(|l| !l.is_empty() || true) // keep all, filter later
        .collect()
}

/// Build an AgentsBlock from extracted inner lines.
fn build_block(raw: String, inner: Vec<String>, start_line: usize, end_line: usize) -> AgentsBlock {
    let mut body = Vec::new();
    let mut related = Vec::new();
    let mut see = Vec::new();
    let mut warnings = Vec::new();

    for line in &inner {
        let t = line.trim();
        if t.is_empty() || t == "@agents" {
            continue;
        }
        if let Some(rest) = t.strip_prefix("Related:") {
            for part in rest.split(',') {
                let p = part.trim().to_string();
                if !p.is_empty() {
                    related.push(p);
                }
            }
        } else if let Some(rest) = t.strip_prefix("See:") {
            for part in rest.split(',') {
                let p = part.trim().to_string();
                if !p.is_empty() {
                    see.push(p);
                }
            }
        } else if t.starts_with("Don't")
            || t.starts_with("Warning:")
            || t.starts_with("Note:")
            || t.starts_with("Avoid:")
        {
            warnings.push(t.to_string());
        } else {
            body.push(t.to_string());
        }
    }

    AgentsBlock {
        raw,
        body,
        related,
        see,
        warnings,
        start_line,
        end_line,
    }
}

/// Generate a comment header string for a given file extension.
#[allow(dead_code)]
pub fn generate_header(block: &AgentsBlock, ext: &str) -> String {
    let style = comment_style(ext);
    let mut lines: Vec<String> = vec!["@agents".to_string()];
    lines.extend(block.body.clone());
    if !block.related.is_empty() {
        lines.push(format!("Related: {}", block.related.join(", ")));
    }
    if !block.see.is_empty() {
        lines.push(format!("See: {}", block.see.join(", ")));
    }
    lines.extend(block.warnings.clone());

    match style {
        CommentStyle::CStyle => {
            let inner: String = lines
                .iter()
                .map(|l| format!(" * {}", l))
                .collect::<Vec<_>>()
                .join("\n");
            format!("/**\n{}\n */", inner)
        }
        CommentStyle::Hash | CommentStyle::Python => lines
            .iter()
            .map(|l| format!("# {}", l))
            .collect::<Vec<_>>()
            .join("\n"),
        CommentStyle::Lua | CommentStyle::Haskell => lines
            .iter()
            .map(|l| format!("-- {}", l))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_parse_js_block_comment() {
        let src = r#"/**
 * @agents
 * OAuth PKCE flow for third-party providers.
 * Uses refresh tokens in httpOnly cookies (see cookie-config.ts).
 * Related: middleware/auth-guard.ts, types/auth.d.ts
 * Don't add session logic here — see session-manager.ts
 */
export function login() {}"#;
        let block = parse_agents_block(src, Path::new("auth.ts")).unwrap();
        assert!(block.body[0].contains("OAuth PKCE"));
        assert_eq!(block.related, vec!["middleware/auth-guard.ts", "types/auth.d.ts"]);
        assert_eq!(block.warnings.len(), 1);
        assert!(block.warnings[0].starts_with("Don't"));
    }

    #[test]
    fn test_parse_python_hash_comment() {
        let src = r#"# @agents
# Main entry point for the data pipeline.
# Related: pipeline/transform.py, pipeline/load.py
# Note: Do not import heavy deps at module level

def run():
    pass"#;
        let block = parse_agents_block(src, Path::new("main.py")).unwrap();
        assert!(block.body[0].contains("Main entry point"));
        assert_eq!(block.related, vec!["pipeline/transform.py", "pipeline/load.py"]);
        assert_eq!(block.warnings.len(), 1);
    }

    #[test]
    fn test_parse_rust_block_comment() {
        let src = r#"/**
 * @agents
 * Graph traversal and reference resolution.
 * Related: parser.rs, cache.rs
 */

pub fn build() {}"#;
        let block = parse_agents_block(src, Path::new("graph.rs")).unwrap();
        assert_eq!(block.related, vec!["parser.rs", "cache.rs"]);
    }

    #[test]
    fn test_no_block_found() {
        let src = "export function foo() {}";
        let result = parse_agents_block(src, Path::new("foo.ts"));
        assert!(result.is_none());
    }

    #[test]
    fn test_block_too_deep() {
        // @agents appears after line 30 — should not be found
        let mut src = String::new();
        for _ in 0..31 {
            src.push_str("// some code\n");
        }
        src.push_str("// @agents\n");
        let result = parse_agents_block(&src, Path::new("foo.ts"));
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_line_comment_ts() {
        let src = r#"// @agents
// Token refresh logic.
// Related: cookie-config.ts
// Avoid: adding session state here

export function refresh() {}"#;
        let block = parse_agents_block(src, Path::new("token-refresh.ts")).unwrap();
        assert!(block.body[0].contains("Token refresh"));
        assert_eq!(block.related, vec!["cookie-config.ts"]);
    }

    #[test]
    fn test_generate_header_ts() {
        let block = AgentsBlock {
            raw: String::new(),
            body: vec!["OAuth PKCE flow.".to_string()],
            related: vec!["auth-guard.ts".to_string()],
            see: vec![],
            warnings: vec!["Don't add session logic here.".to_string()],
            start_line: 1,
            end_line: 5,
        };
        let out = generate_header(&block, "ts");
        assert!(out.contains("/**"));
        assert!(out.contains("@agents"));
        assert!(out.contains("Related: auth-guard.ts"));
    }

    #[test]
    fn test_generate_header_python() {
        let block = AgentsBlock {
            raw: String::new(),
            body: vec!["Pipeline entry.".to_string()],
            related: vec!["transform.py".to_string()],
            see: vec![],
            warnings: vec![],
            start_line: 1,
            end_line: 3,
        };
        let out = generate_header(&block, "py");
        assert!(out.starts_with('#'));
        assert!(out.contains("@agents"));
        assert!(out.contains("Related: transform.py"));
    }

    #[test]
    fn test_generate_header_lua() {
        let block = AgentsBlock {
            raw: String::new(),
            body: vec!["Core module.".to_string()],
            related: vec![],
            see: vec![],
            warnings: vec![],
            start_line: 1,
            end_line: 2,
        };
        let out = generate_header(&block, "lua");
        assert!(out.starts_with("-- @agents"));
    }

    #[test]
    fn test_block_body_no_slash_artifact() {
        // The closing */ should not leak into the body as "/"
        let src = r#"/**
 * @agents
 * Some description.
 */
code here"#;
        let block = parse_agents_block(src, Path::new("foo.ts")).unwrap();
        assert_eq!(block.body, vec!["Some description."]);
        assert!(!block.body.iter().any(|l| l == "/"));
    }

    #[test]
    fn test_parse_see_links() {
        let src = r#"/**
 * @agents
 * Token management.
 * See: https://example.com/docs, internal/spec.md
 */
export function token() {}"#;
        let block = parse_agents_block(src, Path::new("token.ts")).unwrap();
        assert_eq!(block.see, vec!["https://example.com/docs", "internal/spec.md"]);
    }

    #[test]
    fn test_broken_refs_skips_urls() {
        // URL in See: should not be flagged as broken
        // (tested via graph, but parser must preserve See: correctly)
        let src = r#"// @agents
// Auth helper.
// See: https://docs.example.com/auth
"#;
        let block = parse_agents_block(src, Path::new("auth.ts")).unwrap();
        assert_eq!(block.see, vec!["https://docs.example.com/auth"]);
    }

    // --- parse_all_agents_tags tests ---

    #[test]
    fn test_inline_tag_ts() {
        let src = "const x = 1;\n// @agents: Query key must use path, not id.\nconst y = 2;";
        let tags = parse_all_agents_tags(src, Path::new("foo.ts"));
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].kind, TagKind::Inline);
        assert_eq!(tags[0].line, 2);
        assert_eq!(tags[0].text, vec!["Query key must use path, not id."]);
    }

    #[test]
    fn test_inline_tag_multiline_continuation() {
        let src = "code;\n// @agents: First line.\n// Second line continues.\n// Third line.\nmore code;";
        let tags = parse_all_agents_tags(src, Path::new("foo.ts"));
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].text, vec!["First line.", "Second line continues.", "Third line."]);
    }

    #[test]
    fn test_inline_tag_python() {
        let src = "x = 1\n# @agents: Must run before callback.\ny = 2";
        let tags = parse_all_agents_tags(src, Path::new("foo.py"));
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].kind, TagKind::Inline);
        assert_eq!(tags[0].text, vec!["Must run before callback."]);
    }

    #[test]
    fn test_file_header_and_inline_both_found() {
        let src = r#"/**
 * @agents
 * File-level description.
 */

const x = 1;
// @agents: An inline note about x.
const y = 2;"#;
        let tags = parse_all_agents_tags(src, Path::new("foo.ts"));
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].kind, TagKind::FileHeader);
        assert_eq!(tags[1].kind, TagKind::Inline);
        assert_eq!(tags[1].line, 7);
    }

    #[test]
    fn test_consecutive_inline_tags_are_separate() {
        let src = "// @agents: First note.\n// @agents: Second note.\ncode;";
        let tags = parse_all_agents_tags(src, Path::new("foo.ts"));
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].text, vec!["First note."]);
        assert_eq!(tags[1].text, vec!["Second note."]);
        assert_eq!(tags[0].line, 1);
        assert_eq!(tags[1].line, 2);
    }

    #[test]
    fn test_inline_tag_empty_text() {
        let src = "// @agents:\ncode;";
        let tags = parse_all_agents_tags(src, Path::new("foo.ts"));
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].text, vec![""]);
    }

    #[test]
    fn test_inline_tag_beyond_line_30() {
        let mut src = String::new();
        for _ in 0..35 {
            src.push_str("const x = 1;\n");
        }
        src.push_str("// @agents: Late annotation.\n");
        let tags = parse_all_agents_tags(&src, Path::new("foo.ts"));
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].line, 36);
        assert_eq!(tags[0].text, vec!["Late annotation."]);
    }

    #[test]
    fn test_no_tags_returns_empty() {
        let src = "const x = 1;\nfunction foo() {}\n";
        let tags = parse_all_agents_tags(src, Path::new("foo.ts"));
        assert!(tags.is_empty());
    }

    #[test]
    fn test_file_header_lines_excluded_from_inline_scan() {
        // @agents: on line 2 is inside the file-header block — should not produce an Inline tag.
        let src = "/**\n * @agents: should be in header block.\n */\ncode;";
        let tags = parse_all_agents_tags(src, Path::new("foo.ts"));
        // The `@agents:` on line 2 is inside the block comment — will not be treated as inline
        // because the block comment's inner lines use `*` prefix, not `//`.
        // The file-header block (`@agents` without colon) won't match here since it uses `@agents:`.
        // So we expect 0 tags (the `@agents:` form inside /** */ only works if it's an inline form).
        // Actually this tests that the scanner handles `* @agents:` inside a block comment.
        let inline_count = tags.iter().filter(|t| t.kind == TagKind::Inline).collect::<Vec<_>>().len();
        let header_count = tags.iter().filter(|t| t.kind == TagKind::FileHeader).collect::<Vec<_>>().len();
        // The block has `@agents:` (with colon) — parse_agents_block won't find it (needs `@agents` without colon)
        // The inline scanner will find `@agents:` after stripping `*` prefix on line 2.
        // But line 2 is inside the header range if parse_agents_block returned Some.
        // Since parse_agents_block requires `@agents` (no colon), it won't match — so header_range is None.
        // The inline scanner will then find it on line 2 via `*` stripping.
        assert_eq!(header_count, 0);
        assert_eq!(inline_count, 1);
        assert_eq!(tags[0].line, 2);
    }

    #[test]
    fn test_inline_tag_inside_block_comment() {
        let src = r#"/**
 * @agents
 * File description.
 */

function foo() {
    /*
     * @agents: Implementation note about this function.
     */
    return 1;
}"#;
        let tags = parse_all_agents_tags(src, Path::new("foo.ts"));
        let inline: Vec<_> = tags.iter().filter(|t| t.kind == TagKind::Inline).collect();
        assert_eq!(inline.len(), 1);
        assert_eq!(inline[0].text, vec!["Implementation note about this function."]);
    }
}
