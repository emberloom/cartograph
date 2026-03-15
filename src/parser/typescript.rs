use std::path::Path;

use super::ParseResult;

pub fn parse_typescript_source(source: &str, path: &Path) -> ParseResult {
    let is_tsx = path.extension().map(|e| e == "tsx").unwrap_or(false);

    let mut parser = tree_sitter::Parser::new();
    let language = if is_tsx {
        tree_sitter_typescript::LANGUAGE_TSX.into()
    } else {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    };

    if parser.set_language(&language).is_err() {
        tracing::error!("failed to load TypeScript grammar");
        return ParseResult {
            entities: Vec::new(),
            imports: Vec::new(),
            modules: Vec::new(),
        };
    }

    let Some(tree) = parser.parse(source, None) else {
        tracing::warn!(
            "tree-sitter failed to parse TypeScript source ({} bytes)",
            source.len()
        );
        return ParseResult {
            entities: Vec::new(),
            imports: Vec::new(),
            modules: Vec::new(),
        };
    };

    let mut result = ParseResult {
        entities: Vec::new(),
        imports: Vec::new(),
        modules: Vec::new(),
    };

    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        let kind = node.kind();

        if (kind == "import_statement" || kind == "export_statement")
            && let Some(source_node) = node.child_by_field_name("source")
        {
            let raw = &source[source_node.start_byte()..source_node.end_byte()];
            let specifier = raw.trim_matches('"').trim_matches('\'');
            if !specifier.is_empty() {
                result.imports.push(specifier.to_string());
            }
        }

        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return result;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn ts_path() -> &'static Path {
        Path::new("src/foo.ts")
    }
    fn tsx_path() -> &'static Path {
        Path::new("src/foo.tsx")
    }

    #[test]
    fn test_parse_named_import() {
        let src = r#"import { foo } from "./bar.js";"#;
        let result = parse_typescript_source(src, ts_path());
        assert!(
            result.imports.contains(&"./bar.js".to_string()),
            "expected ./bar.js in imports, got: {:?}",
            result.imports
        );
    }

    #[test]
    fn test_parse_default_import() {
        let src = r#"import foo from "./bar.js";"#;
        let result = parse_typescript_source(src, ts_path());
        assert!(
            result.imports.contains(&"./bar.js".to_string()),
            "expected ./bar.js in imports, got: {:?}",
            result.imports
        );
    }

    #[test]
    fn test_parse_type_import() {
        let src = r#"import type { T } from "./types.js";"#;
        let result = parse_typescript_source(src, ts_path());
        assert!(
            result.imports.contains(&"./types.js".to_string()),
            "expected ./types.js in imports, got: {:?}",
            result.imports
        );
    }

    #[test]
    fn test_parse_export_from() {
        let src = r#"export { foo } from "./utils.js";"#;
        let result = parse_typescript_source(src, ts_path());
        assert!(
            result.imports.contains(&"./utils.js".to_string()),
            "expected ./utils.js in imports, got: {:?}",
            result.imports
        );
    }

    #[test]
    fn test_parse_export_star() {
        let src = r#"export * from "./core.js";"#;
        let result = parse_typescript_source(src, ts_path());
        assert!(
            result.imports.contains(&"./core.js".to_string()),
            "expected ./core.js in imports, got: {:?}",
            result.imports
        );
    }

    #[test]
    fn test_parse_export_no_source() {
        let src = r#"export const x = 1;"#;
        let result = parse_typescript_source(src, ts_path());
        assert!(
            result.imports.is_empty(),
            "expected empty imports, got: {:?}",
            result.imports
        );
    }

    #[test]
    fn test_emits_package_import_specifier() {
        // Parser emits non-relative specifiers; mod.rs drops them at resolution time
        let src = r#"import { x } from "vitest";"#;
        let result = parse_typescript_source(src, ts_path());
        assert!(
            result.imports.contains(&"vitest".to_string()),
            "expected vitest in raw imports, got: {:?}",
            result.imports
        );
    }

    #[test]
    fn test_emits_node_protocol_specifier() {
        let src = r#"import fs from "node:fs";"#;
        let result = parse_typescript_source(src, ts_path());
        assert!(
            result.imports.contains(&"node:fs".to_string()),
            "expected node:fs in raw imports, got: {:?}",
            result.imports
        );
    }

    #[test]
    fn test_parse_tsx() {
        let src = r#"import React from "./react.js";
export function App(): JSX.Element { return <div />; }"#;
        let result = parse_typescript_source(src, tsx_path());
        assert!(
            result.imports.contains(&"./react.js".to_string()),
            "expected ./react.js in tsx imports, got: {:?}",
            result.imports
        );
    }

    #[test]
    fn test_modules_always_empty() {
        let src = r#"import { foo } from "./bar.js";"#;
        let result = parse_typescript_source(src, ts_path());
        assert!(
            result.modules.is_empty(),
            "modules must always be empty for TypeScript"
        );
    }

    #[test]
    fn test_entities_always_empty() {
        let src = r#"export class Foo {}"#;
        let result = parse_typescript_source(src, ts_path());
        assert!(result.entities.is_empty(), "entities must be empty in v1");
    }
}
