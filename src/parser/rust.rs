use std::path::Path;
use tree_sitter::Node;

pub struct ParsedEntity {
    pub kind: String,   // "Function", "Struct", "Trait", "Impl"
    pub name: String,
    pub line: usize,
}

pub struct ParseResult {
    pub entities: Vec<ParsedEntity>,
    pub imports: Vec<String>,    // use statements (full path text)
    pub modules: Vec<String>,    // mod declarations (just the name)
}

pub fn parse_rust_source(source: &str, _path: &Path) -> ParseResult {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("failed to load Rust grammar");

    let tree = parser.parse(source, None).expect("failed to parse source");
    let root = tree.root_node();
    let source_bytes = source.as_bytes();

    let mut result = ParseResult {
        entities: Vec::new(),
        imports: Vec::new(),
        modules: Vec::new(),
    };

    walk_node(root, source_bytes, &mut result);

    result
}

fn walk_node(node: Node, source: &[u8], result: &mut ParseResult) {
    match node.kind() {
        "function_item" => {
            if let Some(name) = find_child_text(node, "identifier", source) {
                result.entities.push(ParsedEntity {
                    kind: "Function".to_string(),
                    name,
                    line: node.start_position().row + 1,
                });
            }
        }
        "struct_item" => {
            if let Some(name) = find_child_text(node, "type_identifier", source) {
                result.entities.push(ParsedEntity {
                    kind: "Struct".to_string(),
                    name,
                    line: node.start_position().row + 1,
                });
            }
        }
        "trait_item" => {
            if let Some(name) = find_child_text(node, "type_identifier", source) {
                result.entities.push(ParsedEntity {
                    kind: "Trait".to_string(),
                    name,
                    line: node.start_position().row + 1,
                });
            }
        }
        "impl_item" => {
            if let Some(name) = find_child_text(node, "type_identifier", source) {
                result.entities.push(ParsedEntity {
                    kind: "Impl".to_string(),
                    name,
                    line: node.start_position().row + 1,
                });
            }
        }
        "use_declaration" => {
            // Extract the text of the entire use path subtree (everything after "use ")
            let text = node_text(node, source);
            // Strip leading "use " and trailing ";"
            let trimmed = text
                .trim_start_matches("use ")
                .trim_end_matches(';')
                .trim()
                .to_string();
            result.imports.push(trimmed);
        }
        "mod_item" => {
            // Only collect `mod foo;` declarations, not `mod foo { ... }` blocks
            let has_body = node
                .children(&mut node.walk())
                .any(|c| c.kind() == "declaration_list");
            if !has_body {
                if let Some(name) = find_child_text(node, "identifier", source) {
                    result.modules.push(name);
                }
            }
        }
        _ => {}
    }

    // Recurse into children for all node types
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_node(child, source, result);
    }
}

/// Return the UTF-8 text for a node.
fn node_text(node: Node, source: &[u8]) -> String {
    node.utf8_text(source)
        .unwrap_or("")
        .to_string()
}

/// Find the first child of `node` whose kind matches `kind` and return its text.
fn find_child_text(node: Node, kind: &str, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == kind {
            return Some(node_text(child, source));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_parse_file_extracts_functions() {
        let source = r#"
            pub fn login(user: &str) -> bool { true }
            fn validate(user: &str) -> bool { true }
        "#;
        let result = parse_rust_source(source, Path::new("src/auth.rs"));
        let fns: Vec<_> = result.entities.iter().filter(|e| e.kind == "Function").collect();
        assert_eq!(fns.len(), 2);
        assert!(fns.iter().any(|f| f.name == "login"));
        assert!(fns.iter().any(|f| f.name == "validate"));
    }

    #[test]
    fn test_parse_file_extracts_use_statements() {
        let source = r#"
            use crate::billing;
            use std::collections::HashMap;
        "#;
        let result = parse_rust_source(source, Path::new("src/main.rs"));
        assert!(result.imports.iter().any(|i| i.contains("billing")));
    }

    #[test]
    fn test_parse_file_extracts_mod_declarations() {
        let source = r#"
            mod auth;
            pub mod billing;
        "#;
        let result = parse_rust_source(source, Path::new("src/main.rs"));
        assert!(result.modules.iter().any(|m| m == "auth"));
        assert!(result.modules.iter().any(|m| m == "billing"));
    }

    #[test]
    fn test_parse_file_extracts_structs() {
        let source = r#"
            pub struct Session { pub user: String }
            struct Internal { count: u32 }
        "#;
        let result = parse_rust_source(source, Path::new("src/auth.rs"));
        let structs: Vec<_> = result.entities.iter().filter(|e| e.kind == "Struct").collect();
        assert_eq!(structs.len(), 2);
    }
}
