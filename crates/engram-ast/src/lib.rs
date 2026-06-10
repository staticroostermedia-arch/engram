//! Universal AST-aware source extraction for Engram ingestion via tree-sitter.
//!
//! Parses source code (Rust, Python, TS, JS, Go, Java, C, C++) and produces
//! one [`AstItem`] per public semantic block (function, class, struct, etc.).
//!
//! - **Concept name** = `{file_stem}__{kind}__{item_name}`
//! - **Embed label** = First 2 lines of the block (signature) as semantic distillation
//! - **Full source** = Complete node source for the provlog

use tree_sitter::{Language, Parser, Query, QueryCursor, Node};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemKind {
    Function,
    Struct,
    Enum,
    Trait,
    Impl,
    Class,
    Method,
    Interface,
    Unknown(String),
}

impl ItemKind {
    pub fn as_str(&self) -> &str {
        match self {
            ItemKind::Function => "fn",
            ItemKind::Struct   => "struct",
            ItemKind::Enum     => "enum",
            ItemKind::Trait    => "trait",
            ItemKind::Impl     => "impl",
            ItemKind::Class    => "class",
            ItemKind::Method   => "method",
            ItemKind::Interface=> "interface",
            ItemKind::Unknown(s) => s.as_str(),
        }
    }
}

/// A single extracted semantic item.
#[derive(Debug, Clone)]
pub struct AstItem {
    pub name: String,
    pub kind: ItemKind,
    pub doc_comment: String,
    pub signature: String,
    pub full_source: String,
    pub concept: String,
    // --- PHASE 8.2: EMBODIMENT ---
    pub start_pos: (usize, usize),
    pub end_pos: (usize, usize),
}

impl AstItem {
    pub fn embed_label(&self) -> String {
        if self.doc_comment.is_empty() {
            format!("{}: {}", self.concept, self.signature)
        } else {
            format!("{}: {} — {}", self.concept, self.doc_comment, self.signature)
        }
    }
}

/// Helper to extract preceding contiguous comments.
fn extract_preceding_comments(source_bytes: &[u8], mut node: Node) -> String {
    let mut comments = Vec::new();
    while let Some(prev) = node.prev_sibling() {
        if prev.kind().contains("comment") {
            if let Ok(text) = std::str::from_utf8(&source_bytes[prev.start_byte()..prev.end_byte()]) {
                comments.push(text.trim().to_string());
            }
            node = prev;
        } else {
            break;
        }
    }
    comments.reverse();
    comments.join(" ")
}

/// Common structure to hold parsing configuration for a language
struct LangConfig {
    language: Language,
    query_str: &'static str,
}

impl LangConfig {
    fn new(language: Language, query_str: &'static str) -> Self {
        Self { language, query_str }
    }
}

fn get_config(ext: &str) -> Option<LangConfig> {
    match ext {
        "rs" => Some(LangConfig::new(
            tree_sitter_rust::LANGUAGE.into(),
            r#"
            (function_item name: (identifier) @name) @fn
            (struct_item name: (type_identifier) @name) @struct
            (enum_item name: (type_identifier) @name) @enum
            (trait_item name: (type_identifier) @name) @trait
            (impl_item type: (type_identifier) @name) @impl
            "#
        )),
        "py" => Some(LangConfig::new(
            tree_sitter_python::LANGUAGE.into(),
            r#"
            (function_definition name: (identifier) @name) @fn
            (class_definition name: (identifier) @name) @class
            "#
        )),
        "ts" | "tsx" => Some(LangConfig::new(
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            r#"
            (function_declaration name: (identifier) @name) @fn
            (class_declaration name: (type_identifier) @name) @class
            (interface_declaration name: (type_identifier) @name) @interface
            (method_definition name: (property_identifier) @name) @method
            (lexical_declaration (variable_declarator name: (identifier) @name value: (arrow_function))) @fn
            "#
        )),
        "js" | "jsx" => Some(LangConfig::new(
            tree_sitter_javascript::LANGUAGE.into(),
            r#"
            (function_declaration name: (identifier) @name) @fn
            (class_declaration name: (identifier) @name) @class
            (method_definition name: (property_identifier) @name) @method
            (lexical_declaration (variable_declarator name: (identifier) @name value: (arrow_function))) @fn
            "#
        )),
        "go" => Some(LangConfig::new(
            tree_sitter_go::LANGUAGE.into(),
            r#"
            (function_declaration name: (identifier) @name) @fn
            (method_declaration name: (field_identifier) @name) @method
            (type_declaration (type_spec name: (type_identifier) @name type: (struct_type))) @struct
            (type_declaration (type_spec name: (type_identifier) @name type: (interface_type))) @interface
            "#
        )),
        "java" => Some(LangConfig::new(
            tree_sitter_java::LANGUAGE.into(),
            r#"
            (class_declaration name: (identifier) @name) @class
            (interface_declaration name: (identifier) @name) @interface
            (method_declaration name: (identifier) @name) @method
            "#
        )),
        "c" => Some(LangConfig::new(
            tree_sitter_c::LANGUAGE.into(),
            r#"
            (function_definition declarator: (function_declarator declarator: (identifier) @name)) @fn
            (struct_specifier name: (type_identifier) @name) @struct
            (enum_specifier name: (type_identifier) @name) @enum
            "#
        )),
        "cpp" | "cc" | "cxx" | "h" | "hpp" => Some(LangConfig::new(
            tree_sitter_cpp::LANGUAGE.into(),
            r#"
            (function_definition declarator: (function_declarator declarator: (identifier) @name)) @fn
            (function_definition declarator: (function_declarator declarator: (field_identifier) @name)) @method
            (class_specifier name: (type_identifier) @name) @class
            (struct_specifier name: (type_identifier) @name) @struct
            (enum_specifier name: (type_identifier) @name) @enum
            "#
        )),
        _ => None,
    }
}

/// Extract all supported AST items from the given source text based on the file extension.
pub fn extract_ast_items(file_path: &str, source: &str) -> Vec<AstItem> {
    let ext = std::path::Path::new(file_path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    // Special passive support for Markdown (no tree-sitter grammar in this crate yet).
    // Treat top-level headings (#, ##, etc.) as "section" items + code fences as sub-items with AABB.
    // This makes docs/plan/README/skills ingestion first-class. Enhanced 2026-06 for passive spatial.
    if ext == "md" {
        return extract_md_structure(file_path, source);
    }

    // Passive structured support for TOML (processes/*.toml, Cargo.toml, configs) — no external ts-toml dep.
    // Tables [foo] and [[bar]] become items with line AABB for full context/recall_in_file on declarative process sheaf.
    if ext == "toml" {
        return extract_toml_structure(file_path, source);
    }

    let config = match get_config(ext) {
        Some(c) => c,
        None => return Vec::new(),
    };

    let mut parser = Parser::new();
    if parser.set_language(&config.language).is_err() {
        return Vec::new();
    }

    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return Vec::new(),
    };

    let query = match Query::new(&config.language, config.query_str) {
        Ok(q) => q,
        Err(_) => return Vec::new(),
    };

    let mut cursor = QueryCursor::new();
    let matches = cursor.matches(&query, tree.root_node(), source.as_bytes());

    let mut items = Vec::new();
    let file_stem = std::path::Path::new(file_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_lowercase()
        .replace(|c: char| !c.is_alphanumeric(), "_");

    let source_bytes = source.as_bytes();

    for m in matches {
        let mut name = String::new();
        let mut kind = ItemKind::Unknown("item".to_string());
        let mut node_opt = None;

        for capture in m.captures {
            let capture_name = query.capture_names()[capture.index as usize];
            if capture_name == "name" {
                if let Ok(n) = std::str::from_utf8(&source_bytes[capture.node.start_byte()..capture.node.end_byte()]) {
                    name = n.to_string();
                }
            } else {
                // Must be one of the item captures: @fn, @class, @struct, etc.
                kind = match capture_name {
                    "fn" => ItemKind::Function,
                    "class" => ItemKind::Class,
                    "struct" => ItemKind::Struct,
                    "enum" => ItemKind::Enum,
                    "trait" => ItemKind::Trait,
                    "impl" => ItemKind::Impl,
                    "method" => ItemKind::Method,
                    "interface" => ItemKind::Interface,
                    s => ItemKind::Unknown(s.to_string()),
                };
                node_opt = Some(capture.node);
            }
        }

        if let Some(node) = node_opt {
            if !name.is_empty() {
                let start = node.start_byte();
                let end = node.end_byte();
                
                // Extract 2D coordinates for the AABB mapping
                let start_pos = (node.start_position().row, node.start_position().column);
                let end_pos = (node.end_position().row, node.end_position().column);

                let full_source = std::str::from_utf8(&source_bytes[start..end]).unwrap_or("").to_string();

                // Signature is roughly the first line or two of the node
                let first_newline = full_source.find('\n').unwrap_or(full_source.len());
                let mut sig_end = first_newline;
                if sig_end < full_source.len() - 1 && full_source.chars().filter(|c| *c == '{').count() > 0 {
                    if let Some(brace) = full_source.find('{') {
                         sig_end = sig_end.max(brace + 1);
                    }
                }
                let signature = full_source[0..sig_end.min(full_source.len())].replace('\n', " ").trim().to_string();

                let doc_comment = extract_preceding_comments(source_bytes, node);

                let concept = format!("{}__{}__{}", file_stem, kind.as_str(), name)
                    .to_lowercase()
                    .replace(|c: char| !c.is_alphanumeric() && c != '_', "_");

                items.push(AstItem {
                    name,
                    kind,
                    doc_comment,
                    signature,
                    full_source,
                    concept,
                    start_pos,
                    end_pos,
                });
            }
        }
    }

    items
}

/// Enhanced passive MD "AST" (no external ts-md dep).
/// Produces section items per # heading (level 1-6) + code fence items for ``` blocks.
/// Frontmatter (leading --- ... ---) captured as special item.
/// AABB line-based. Enables rich spatial for skills, plans, AGENTS.md, examples without crude chunking.
/// This + toml makes passive ingestion effective for non-rs workspace files we actually edit.
fn extract_md_structure(file_path: &str, source: &str) -> Vec<AstItem> {
    let mut items = Vec::new();
    let file_stem = std::path::Path::new(file_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_lowercase()
        .replace(|c: char| !c.is_alphanumeric(), "_");

    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;

    // Capture leading frontmatter as special item (common in skills/*.md, some docs)
    if !lines.is_empty() && lines[0].trim() == "---" {
        let mut end = 1;
        while end < lines.len() {
            if lines[end].trim() == "---" {
                end += 1;
                break;
            }
            end += 1;
        }
        let full_source = lines[0..end].join("\n");
        let start_pos = (0, 0);
        let end_pos = (end.saturating_sub(1), lines[end.saturating_sub(1)].len());
        let concept = format!("{}__frontmatter", file_stem);
        items.push(AstItem {
            name: "frontmatter".to_string(),
            kind: ItemKind::Unknown("frontmatter".to_string()),
            doc_comment: String::new(),
            signature: "--- frontmatter ---".to_string(),
            full_source,
            concept,
            start_pos,
            end_pos,
        });
        i = end;
    }

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();

        // Headings
        if trimmed.starts_with('#') {
            let level = trimmed.chars().take_while(|&c| c == '#').count();
            if level > 0 && level <= 6 {
                let name = trimmed[level..].trim().to_string();
                if !name.is_empty() {
                    let start_line = i;
                    let mut end_line = i + 1;
                    while end_line < lines.len() {
                        let next = lines[end_line].trim_start();
                        if next.starts_with('#') {
                            let next_level = next.chars().take_while(|&c| c == '#').count();
                            if next_level <= level {
                                break;
                            }
                        }
                        end_line += 1;
                    }
                    let full_source = lines[start_line..end_line].join("\n");
                    let start_pos = (start_line, 0);
                    let end_pos = (end_line.saturating_sub(1), lines[end_line.saturating_sub(1)].len());
                    let concept = format!("{}__section__{}", file_stem, name)
                        .to_lowercase()
                        .replace(|c: char| !c.is_alphanumeric() && c != '_', "_");
                    let signature = name.clone();
                    items.push(AstItem {
                        name,
                        kind: ItemKind::Unknown(format!("h{}", level)),
                        doc_comment: String::new(),
                        signature,
                        full_source,
                        concept,
                        start_pos,
                        end_pos,
                    });
                }
            }
        }

        // Code fences ```lang or ``` 
        if let Some(stripped) = trimmed.strip_prefix("```") {
            let lang = stripped.trim().to_string();
            let start_line = i;
            let mut end_line = i + 1;
            while end_line < lines.len() {
                if lines[end_line].trim_start().starts_with("```") {
                    end_line += 1;
                    break;
                }
                end_line += 1;
            }
            let full_source = lines[start_line..end_line].join("\n");
            let start_pos = (start_line, 0);
            let end_pos = (end_line.saturating_sub(1), lines[end_line.saturating_sub(1)].len());
            let code_name = if lang.is_empty() { "code".to_string() } else { lang.clone() };
            let concept = format!("{}__code__{}", file_stem, code_name)
                .to_lowercase()
                .replace(|c: char| !c.is_alphanumeric() && c != '_', "_");
            items.push(AstItem {
                name: code_name,
                kind: ItemKind::Unknown("code".to_string()),
                doc_comment: String::new(),
                signature: format!("``` {}", lang).trim().to_string(),
                full_source,
                concept,
                start_pos,
                end_pos,
            });
        }

        i += 1;
    }
    items
}

/// Lightweight passive TOML structure extractor (no tree-sitter-toml or toml crate dep).
/// Treats [table], [[array-of-tables]], and top-level bare keys as items with line AABB.
/// Enables rich spatial AABB for processes/*.toml (rituals, subvisor), pyproject etc without fallback chunk.
/// Name sanitizes for concept; full section/key block as content. Robust for our declarative process sheaf.
fn extract_toml_structure(file_path: &str, source: &str) -> Vec<AstItem> {
    let mut items = Vec::new();
    let file_stem = std::path::Path::new(file_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_lowercase()
        .replace(|c: char| !c.is_alphanumeric(), "_");

    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();
        if line.is_empty() || line.starts_with('#') {
            i += 1;
            continue;
        }

        // Array of tables [[name]]
        if line.starts_with("[[") && line.ends_with("]]") {
            let name = line[2..line.len()-2].trim().to_string();
            let mut end_line = i + 1;
            if !name.is_empty() {
                let start_line = i;
                while end_line < lines.len() {
                    let next = lines[end_line].trim();
                    if (next.starts_with('[') && !next.starts_with("[[")) || next.starts_with("[[") {
                        break;
                    }
                    end_line += 1;
                }
                let full_source = lines[start_line..end_line].join("\n");
                let concept = format!("{}__table__{}", file_stem, name)
                    .to_lowercase()
                    .replace(|c: char| !c.is_alphanumeric() && c != '_', "_");
                items.push(AstItem {
                    name: name.clone(),
                    kind: ItemKind::Unknown("array_table".to_string()),
                    doc_comment: String::new(),
                    signature: line.to_string(),
                    full_source,
                    concept,
                    start_pos: (start_line, 0),
                    end_pos: (end_line.saturating_sub(1), lines[end_line.saturating_sub(1)].len()),
                });
            }
            i = end_line;
            continue;
        }

        // Table [name] or [name.sub]
        if line.starts_with('[') && line.ends_with(']') && !line.starts_with("[[") {
            let name = line[1..line.len()-1].trim().to_string();
            let mut end_line = i + 1;
            if !name.is_empty() {
                let start_line = i;
                while end_line < lines.len() {
                    let next = lines[end_line].trim();
                    if next.starts_with('[') {
                        break;
                    }
                    end_line += 1;
                }
                let full_source = lines[start_line..end_line].join("\n");
                let concept = format!("{}__table__{}", file_stem, name)
                    .to_lowercase()
                    .replace(|c: char| !c.is_alphanumeric() && c != '_', "_");
                items.push(AstItem {
                    name: name.clone(),
                    kind: ItemKind::Unknown("table".to_string()),
                    doc_comment: String::new(),
                    signature: line.to_string(),
                    full_source,
                    concept,
                    start_pos: (start_line, 0),
                    end_pos: (end_line.saturating_sub(1), lines[end_line.saturating_sub(1)].len()),
                });
            }
            i = end_line;
            continue;
        }

        // Bare top-level key = value (simple key items for small tomls)
        if let Some(eq) = line.find('=') {
            let key = line[..eq].trim().to_string();
            if !key.is_empty() && !key.contains('[') && !key.contains('.') {  // top level only, rough
                let start_line = i;
                let end_line = i + 1;
                let full_source = lines[start_line..end_line].join("\n");
                let concept = format!("{}__key__{}", file_stem, key)
                    .to_lowercase()
                    .replace(|c: char| !c.is_alphanumeric() && c != '_', "_");
                items.push(AstItem {
                    name: key.clone(),
                    kind: ItemKind::Unknown("key".to_string()),
                    doc_comment: String::new(),
                    signature: line.to_string(),
                    full_source,
                    concept,
                    start_pos: (start_line, 0),
                    end_pos: (end_line.saturating_sub(1), lines[end_line.saturating_sub(1)].len()),
                });
            }
        }

        i += 1;
    }
    items
}
