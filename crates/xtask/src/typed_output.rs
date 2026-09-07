//! Enforce named command output contracts by rejecting production `json!`
//! calls. Parse Rust syntax instead of truncating at the first test module:
//! production dispatch handlers frequently follow inline tests in this repo.

use anyhow::{Context, Result};
use proc_macro2::{TokenStream, TokenTree};
use std::path::{Path, PathBuf};
use syn::visit::{self, Visit};

use crate::workspace::workspace_root;

pub fn run() -> Result<bool> {
    let root = workspace_root()?;
    let entry = root.join("crates/hyalo-cli/src/commands/mod.rs");
    let mut violations = Vec::new();
    inspect_file(&entry, &mut violations)?;
    for path in &violations {
        eprintln!(
            "check-typed-output: production json! call in {}",
            path.display()
        );
    }
    if violations.is_empty() {
        println!("check-typed-output: production command modules use no json! macros");
    }
    Ok(violations.is_empty())
}

/// Only an explicit test-only condition excludes a node. Unknown cfg values
/// remain eligible, so platform/feature-gated production cannot escape checks.
fn test_only(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .parse_args::<syn::Meta>()
                .is_ok_and(|meta| requires_test(&meta))
    })
}

fn requires_test(meta: &syn::Meta) -> bool {
    match meta {
        syn::Meta::Path(path) => path.is_ident("test"),
        syn::Meta::List(list) if list.path.is_ident("all") || list.path.is_ident("any") => {
            use syn::parse::Parser as _;
            let parser = syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated;
            let Ok(items) = parser.parse2(list.tokens.clone()) else {
                return false;
            };
            if list.path.is_ident("all") {
                items.iter().any(requires_test)
            } else {
                !items.is_empty() && items.iter().all(requires_test)
            }
        }
        _ => false,
    }
}

fn inspect_file(path: &Path, violations: &mut Vec<PathBuf>) -> Result<()> {
    let source =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let file = syn::parse_file(&source).with_context(|| format!("parsing {}", path.display()))?;
    let parent = path.parent().context("command source has no parent")?;
    let module_dir = if path.file_stem().is_some_and(|s| s == "mod") {
        parent.to_path_buf()
    } else {
        parent.join(path.file_stem().context("command source has no stem")?)
    };
    inspect_items(&file.items, path, &module_dir, violations)
}

fn inspect_items(
    items: &[syn::Item],
    path: &Path,
    module_dir: &Path,
    violations: &mut Vec<PathBuf>,
) -> Result<()> {
    for item in items {
        if let syn::Item::Mod(module) = item {
            if test_only(&module.attrs) {
                continue;
            }
            if let Some((_, items)) = &module.content {
                inspect_items(
                    items,
                    path,
                    &module_dir.join(module.ident.to_string()),
                    violations,
                )?;
            } else {
                let explicit = module.attrs.iter().find_map(|attr| {
                    if !attr.path().is_ident("path") {
                        return None;
                    }
                    let syn::Meta::NameValue(value) = &attr.meta else {
                        return None;
                    };
                    let syn::Expr::Lit(lit) = &value.value else {
                        return None;
                    };
                    let syn::Lit::Str(value) = &lit.lit else {
                        return None;
                    };
                    Some(module_dir.join(value.value()))
                });
                let flat = module_dir.join(format!("{}.rs", module.ident));
                let child = explicit.unwrap_or_else(|| {
                    if flat.is_file() {
                        flat
                    } else {
                        module_dir.join(module.ident.to_string()).join("mod.rs")
                    }
                });
                inspect_file(&child, violations)?;
            }
        } else {
            let mut visitor = ProductionMacros { count: 0 };
            visitor.visit_item(item);
            violations.extend(std::iter::repeat_n(path.to_path_buf(), visitor.count));
        }
    }
    Ok(())
}

#[derive(Default)]
struct ProductionMacros {
    count: usize,
}

impl<'ast> Visit<'ast> for ProductionMacros {
    fn visit_item(&mut self, item: &'ast syn::Item) {
        // syn does not expose a common attrs accessor for Item.
        let attrs = match item {
            syn::Item::Fn(v) => &v.attrs,
            syn::Item::Const(v) => &v.attrs,
            syn::Item::Static(v) => &v.attrs,
            syn::Item::Impl(v) => &v.attrs,
            syn::Item::Mod(v) => &v.attrs,
            syn::Item::Macro(v) => &v.attrs,
            syn::Item::Trait(v) => &v.attrs,
            _ => return visit::visit_item(self, item),
        };
        if !test_only(attrs) {
            visit::visit_item(self, item);
        }
    }
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        if !test_only(&item.attrs) {
            visit::visit_impl_item_fn(self, item);
        }
    }
    fn visit_local(&mut self, local: &'ast syn::Local) {
        if !test_only(&local.attrs) {
            visit::visit_local(self, local);
        }
    }
    fn visit_stmt_macro(&mut self, stmt: &'ast syn::StmtMacro) {
        if !test_only(&stmt.attrs) {
            visit::visit_stmt_macro(self, stmt);
        }
    }
    fn visit_expr_macro(&mut self, expr: &'ast syn::ExprMacro) {
        if !test_only(&expr.attrs) {
            visit::visit_expr_macro(self, expr);
        }
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        if mac.path.segments.last().is_some_and(|s| s.ident == "json") {
            self.count += 1;
        } else {
            self.count += nested_json_calls(mac.tokens.clone());
        }
    }
}

/// Macro arguments are token streams rather than AST expressions. Inspect
/// groups so `vec![serde_json::json!({})]` is covered too, without matching
/// string literals, comments, or documentation containing example code.
fn nested_json_calls(tokens: TokenStream) -> usize {
    let tokens: Vec<_> = tokens.into_iter().collect();
    let direct = tokens
        .windows(2)
        .filter(|pair| {
            matches!(&pair[0], TokenTree::Ident(i) if i == "json")
                && matches!(&pair[1], TokenTree::Punct(p) if p.as_char() == '!')
        })
        .count();
    direct
        + tokens
            .into_iter()
            .filter_map(|token| match token {
                TokenTree::Group(group) => Some(nested_json_calls(group.stream())),
                _ => None,
            })
            .sum::<usize>()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(source: &str) -> usize {
        let file = syn::parse_file(source).unwrap();
        let mut visitor = ProductionMacros::default();
        visitor.visit_file(&file);
        visitor.count
    }

    #[test]
    fn excludes_only_test_nodes_and_keeps_later_production() {
        assert_eq!(
            count(
                r#"
            #[cfg(test)] mod tests { fn check() { serde_json::json!({}); } }
            fn production() { serde_json :: json ! ({}); }
        "#
            ),
            1
        );
        assert_eq!(
            count(
                r#"
            fn production() {
                #[cfg(test)] let old = serde_json::json!({});
                let output = vec![serde_json::json!({})];
            }
        "#
            ),
            1
        );
        assert_eq!(
            count(
                r#"
            #[cfg(any(test, windows))] fn production() { json!({}); }
            #[cfg(all(test, unix))] fn tests() { json!({}); }
            const EXAMPLE: &str = "json!({})";
            // json!({})
        "#
            ),
            1
        );
    }

    #[test]
    fn follows_production_modules_without_scanning_test_files() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("mod.rs"),
            "#[cfg(test)] mod tests; mod child;",
        )
        .unwrap();
        std::fs::write(tmp.path().join("tests.rs"), "fn f() { json!({}); }").unwrap();
        std::fs::write(tmp.path().join("child.rs"), "fn f() { json!({}); }").unwrap();
        let mut violations = Vec::new();
        inspect_file(&tmp.path().join("mod.rs"), &mut violations).unwrap();
        assert_eq!(violations, [tmp.path().join("child.rs")]);
    }
}
