//! Import extraction utilities for different programming languages

use scribe_core::Language;
use std::collections::HashSet;

#[cfg(feature = "analysis")]
use scribe_analysis::ast_import_parser::{ImportLanguage as AstImportLanguage, SimpleAstParser};

/// Extract imports from file content based on language
pub fn extract_imports_for_diff(content: &str, language: &Language) -> Vec<String> {
    let mut imports = HashSet::new();

    match language {
        Language::Rust => extract_rust_imports(content, &mut imports),
        Language::Python => extract_python_imports(content, &mut imports),
        Language::JavaScript | Language::TypeScript => extract_js_imports(content, &mut imports),
        Language::Go => extract_go_imports(content, &mut imports),
        Language::Elixir => extract_elixir_imports(content, &mut imports),
        _ => {}
    }

    let mut ordered: Vec<String> = imports.into_iter().collect();
    ordered.sort();
    ordered.truncate(64);
    ordered
}

pub fn extract_rust_imports(content: &str, imports: &mut HashSet<String>) {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("use ") {
            let statement = trimmed
                .trim_start_matches("use ")
                .trim_end_matches(';')
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .trim_end_matches("::");
            if !statement.is_empty() {
                imports.insert(statement.to_string());
            }
        } else if trimmed.starts_with("mod ") {
            let module = trimmed
                .trim_start_matches("mod ")
                .trim_end_matches(';')
                .trim();
            if !module.is_empty() {
                imports.insert(module.to_string());
            }
        }
    }
}

pub fn extract_python_imports(content: &str, imports: &mut HashSet<String>) {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("import ") {
            for module in trimmed.trim_start_matches("import ").split(',') {
                let module = module.trim().split_whitespace().next().unwrap_or("");
                if !module.is_empty() {
                    imports.insert(module.to_string());
                }
            }
        } else if trimmed.starts_with("from ") && trimmed.contains(" import ") {
            let module = trimmed
                .trim_start_matches("from ")
                .split(" import ")
                .next()
                .unwrap_or("")
                .trim();
            if !module.is_empty() {
                imports.insert(module.to_string());
            }
        }
    }
}

pub fn extract_js_imports(content: &str, imports: &mut HashSet<String>) {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("import ") {
            if let Some(start) = trimmed.find('"') {
                if let Some(end) = trimmed[start + 1..].find('"') {
                    imports.insert(trimmed[start + 1..start + 1 + end].to_string());
                }
            } else if let Some(start) = trimmed.find('\'') {
                if let Some(end) = trimmed[start + 1..].find('\'') {
                    imports.insert(trimmed[start + 1..start + 1 + end].to_string());
                }
            }
        } else if trimmed.contains("require(") {
            if let Some(start) = trimmed.find("require(") {
                let start = start + "require(".len();
                let slice = &trimmed[start..];
                if let Some(end_idx) = slice.find(')') {
                    let inner = &slice[..end_idx];
                    let inner = inner.trim_matches(&['\'', '"'][..]);
                    if !inner.is_empty() {
                        imports.insert(inner.to_string());
                    }
                }
            }
        }
    }
}

pub fn extract_go_imports(content: &str, imports: &mut HashSet<String>) {
    let mut in_block = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "import (" {
            in_block = true;
            continue;
        }
        if in_block {
            if trimmed == ")" {
                in_block = false;
                continue;
            }
            let import_path = trimmed.trim_matches(&['"', '`'][..]);
            if !import_path.is_empty() {
                imports.insert(import_path.to_string());
            }
        } else if trimmed.starts_with("import ") {
            let import_path = trimmed
                .trim_start_matches("import ")
                .trim_matches(&['"', '`'][..]);
            if !import_path.is_empty() {
                imports.insert(import_path.to_string());
            }
        }
    }
}

/// Extract Elixir imports (`alias`, `import`, `require`, `use`) from content
pub fn extract_elixir_imports(content: &str, imports: &mut HashSet<String>) {
    if let Some(ast_imports) = extract_elixir_imports_with_ast(content) {
        imports.extend(ast_imports);
        return;
    }

    extract_elixir_imports_line_based(content, imports);
}

#[cfg(feature = "analysis")]
fn extract_elixir_imports_with_ast(content: &str) -> Option<Vec<String>> {
    let parser = SimpleAstParser::new().ok()?;
    let imports = parser
        .extract_imports(content, AstImportLanguage::Elixir)
        .ok()?;

    Some(imports.into_iter().map(|import| import.module).collect())
}

#[cfg(not(feature = "analysis"))]
fn extract_elixir_imports_with_ast(_content: &str) -> Option<Vec<String>> {
    None
}

fn extract_elixir_imports_line_based(content: &str, imports: &mut HashSet<String>) {
    let mut heredoc_state: Option<ElixirHeredocDelimiter> = None;

    for line in content.lines() {
        let without_heredocs = strip_elixir_heredocs_from_line(line, &mut heredoc_state);
        let without_comments = without_heredocs.split('#').next().unwrap_or("").trim();
        if without_comments.is_empty() {
            continue;
        }

        for keyword in ["alias ", "import ", "require ", "use "] {
            if let Some(statement) = without_comments.strip_prefix(keyword) {
                extract_elixir_import_statement(statement, imports);
                break;
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ElixirHeredocDelimiter {
    TripleDouble,
    TripleSingle,
}

impl ElixirHeredocDelimiter {
    fn token(self) -> &'static str {
        match self {
            ElixirHeredocDelimiter::TripleDouble => "\"\"\"",
            ElixirHeredocDelimiter::TripleSingle => "'''",
        }
    }
}

fn strip_elixir_heredocs_from_line(
    line: &str,
    heredoc_state: &mut Option<ElixirHeredocDelimiter>,
) -> String {
    let mut output = String::new();
    let mut cursor = line;

    loop {
        if let Some(active_delimiter) = *heredoc_state {
            if let Some(end_index) = cursor.find(active_delimiter.token()) {
                cursor = &cursor[end_index + active_delimiter.token().len()..];
                *heredoc_state = None;
                continue;
            }

            return output;
        }

        let next_double = cursor.find("\"\"\"");
        let next_single = cursor.find("'''");

        let next_delimiter = match (next_double, next_single) {
            (Some(double_idx), Some(single_idx)) if double_idx <= single_idx => {
                Some((double_idx, ElixirHeredocDelimiter::TripleDouble))
            }
            (Some(_), Some(single_idx)) => Some((single_idx, ElixirHeredocDelimiter::TripleSingle)),
            (Some(double_idx), None) => Some((double_idx, ElixirHeredocDelimiter::TripleDouble)),
            (None, Some(single_idx)) => Some((single_idx, ElixirHeredocDelimiter::TripleSingle)),
            (None, None) => None,
        };

        let Some((start_index, delimiter)) = next_delimiter else {
            output.push_str(cursor);
            break;
        };

        output.push_str(&cursor[..start_index]);
        cursor = &cursor[start_index + delimiter.token().len()..];

        if let Some(end_index) = cursor.find(delimiter.token()) {
            cursor = &cursor[end_index + delimiter.token().len()..];
            continue;
        }

        *heredoc_state = Some(delimiter);
        break;
    }

    output
}

fn extract_elixir_import_statement(statement: &str, imports: &mut HashSet<String>) {
    if let Some((base, remainder)) = statement.split_once('{') {
        let base = normalize_elixir_module(base.trim_end_matches('.'));
        if let Some(end) = remainder.find('}') {
            let grouped = &remainder[..end];
            for module in grouped.split(',') {
                if let Some(module) = normalize_elixir_module(module) {
                    if let Some(ref base) = base {
                        imports.insert(format!("{}.{}", base, module));
                    } else {
                        imports.insert(module);
                    }
                }
            }
        }
        return;
    }

    if let Some(module) = normalize_elixir_module(statement) {
        imports.insert(module);
    }
}

fn normalize_elixir_module(raw: &str) -> Option<String> {
    let mut module = raw.trim();

    if let Some((before_options, _)) = module.split_once(',') {
        module = before_options.trim();
    }

    if module.ends_with(" do") {
        module = module.trim_end_matches(" do").trim_end();
    }

    module = module.trim_matches(|c: char| matches!(c, '"' | '\'' | '(' | ')'));

    if let Some(stripped) = module.strip_prefix("Elixir.") {
        module = stripped;
    }

    let cleaned: String = module
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.')
        .collect();
    let cleaned = cleaned.trim_end_matches('.').to_string();

    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_imports_for_diff_elixir() {
        let content = r#"
alias MyApp.Repo
alias MyApp.{Accounts.User, Accounts.Team}
import Plug.Conn
require Logger
use MyAppWeb, :controller
"#;

        let imports = extract_imports_for_diff(content, &Language::Elixir);

        assert!(imports.contains(&"MyApp.Repo".to_string()));
        assert!(imports.contains(&"MyApp.Accounts.User".to_string()));
        assert!(imports.contains(&"MyApp.Accounts.Team".to_string()));
        assert!(imports.contains(&"Plug.Conn".to_string()));
        assert!(imports.contains(&"Logger".to_string()));
        assert!(imports.contains(&"MyAppWeb".to_string()));
    }

    #[test]
    fn test_extract_imports_for_diff_elixir_ignores_options() {
        let content = r#"
alias MyApp.Accounts.User, as: AccountUser
import Plug.Conn, only: [put_status: 2]
require Logger, as: AppLogger
use Phoenix.Controller, namespace: MyAppWeb
"#;

        let imports = extract_imports_for_diff(content, &Language::Elixir);

        assert!(imports.contains(&"MyApp.Accounts.User".to_string()));
        assert!(imports.contains(&"Plug.Conn".to_string()));
        assert!(imports.contains(&"Logger".to_string()));
        assert!(imports.contains(&"Phoenix.Controller".to_string()));
        assert!(!imports.contains(&"AccountUser".to_string()));
        assert!(!imports.contains(&"AppLogger".to_string()));
        assert!(!imports.contains(&"MyAppWeb".to_string()));
    }

    #[test]
    fn test_extract_imports_for_diff_elixir_ignores_comments_and_strings() {
        let content = r#"
# alias Fake.Module
text = "alias Hidden.Module"
doc = """
import Not.Real
"""
alias MyApp.Repo
"#;

        let imports = extract_imports_for_diff(content, &Language::Elixir);

        assert!(imports.contains(&"MyApp.Repo".to_string()));
        assert!(!imports.contains(&"Fake.Module".to_string()));
        assert!(!imports.contains(&"Hidden.Module".to_string()));
        assert!(!imports.contains(&"Not.Real".to_string()));
    }

    #[test]
    fn test_elixir_line_fallback_ignores_heredoc_imports() {
        let content = r#"
alias MyApp.Repo

doc = """
import Not.Real
alias Also.Not.Real
"""

notes = '''
use Another.Not.Real
'''

require Logger
"#;

        let mut imports = HashSet::new();
        extract_elixir_imports_line_based(content, &mut imports);

        assert!(imports.contains("MyApp.Repo"));
        assert!(imports.contains("Logger"));
        assert!(!imports.contains("Not.Real"));
        assert!(!imports.contains("Also.Not.Real"));
        assert!(!imports.contains("Another.Not.Real"));
    }
}
