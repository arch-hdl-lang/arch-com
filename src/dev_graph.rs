//! Curated feature-to-compiler graph with parser-verified Rust symbol locations.

use proc_macro2::Span;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use syn::{ImplItem, Item};

const SCHEMA_VERSION: u32 = 1;

fn generator_id() -> String {
    format!(
        "arch {} graph {}",
        env!("CARGO_PKG_VERSION"),
        option_env!("ARCH_GRAPH_GENERATOR_ID").unwrap_or("unavailable")
    )
}

#[derive(Debug, Deserialize)]
struct FeatureMap {
    schema_version: u32,
    #[serde(default)]
    common_links: Vec<Link>,
    features: Vec<Feature>,
}

#[derive(Debug, Deserialize)]
struct Feature {
    name: String,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    covers: Vec<String>,
    links: Vec<Link>,
}

#[derive(Debug, Deserialize)]
struct Link {
    role: String,
    path: String,
    #[serde(default)]
    symbol: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DevNode {
    pub id: String,
    pub kind: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub covers: Vec<String>,
    pub path: String,
    pub line: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DevEdge {
    pub from: String,
    pub to: String,
    pub role: String,
    pub provenance: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    schema_version: u32,
    generator: String,
    source_revision: Option<String>,
    sources: BTreeMap<String, String>,
}

#[derive(Debug, Default)]
pub struct DevStats {
    pub features: usize,
    pub nodes: usize,
    pub edges: usize,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

pub(crate) fn digest(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(0xcbf29ce484222325u64, |acc, byte| {
        (acc ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    });
    format!("{hash:016x}")
}

fn revision(root: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn symbol_line(
    path: &Path,
    symbol: &str,
    symbols: &BTreeMap<String, Vec<usize>>,
) -> io::Result<usize> {
    match symbols.get(symbol).map(Vec::as_slice) {
        Some([line]) => Ok(*line),
        Some(lines) if !lines.is_empty() => Err(invalid(format!(
            "{}: symbol {symbol} is ambiguous at lines {lines:?}",
            path.display()
        ))),
        _ => Err(invalid(format!(
            "{}: Rust symbol {symbol} was not found",
            path.display()
        ))),
    }
}

fn ast_variants(source: &str) -> io::Result<BTreeMap<String, BTreeSet<String>>> {
    let syntax =
        syn::parse_file(source).map_err(|err| invalid(format!("AST Rust parse failed: {err}")))?;
    let mut enums = BTreeMap::new();
    for item in &syntax.items {
        if let Item::Enum(value) = item {
            let mut variants = BTreeSet::new();
            for variant in &value.variants {
                let name = format!("{}::{}", value.ident, variant.ident);
                if !variants.insert(name.clone()) {
                    return Err(invalid(format!("duplicate AST variant {name}")));
                }
            }
            if enums.insert(value.ident.to_string(), variants).is_some() {
                return Err(invalid(format!("ambiguous AST enum {}", value.ident)));
            }
        }
    }
    Ok(enums)
}

fn validate_coverage(
    map: &FeatureMap,
    variants: &BTreeMap<String, BTreeSet<String>>,
) -> io::Result<()> {
    let known: BTreeSet<_> = variants.values().flatten().collect();
    let mut owners = BTreeMap::new();
    for feature in &map.features {
        for covered in &feature.covers {
            if !known.contains(covered) {
                return Err(invalid(format!(
                    "{}: unknown AST coverage {covered}",
                    feature.name
                )));
            }
            if let Some(previous) = owners.insert(covered, &feature.name) {
                return Err(invalid(format!(
                    "duplicate AST coverage {covered}: {previous} and {}",
                    feature.name
                )));
            }
        }
    }
    Ok(())
}

fn add_symbol(symbols: &mut BTreeMap<String, Vec<usize>>, name: String, span: Span) {
    symbols.entry(name).or_default().push(span.start().line);
}

fn collect_symbols(items: &[Item], symbols: &mut BTreeMap<String, Vec<usize>>) {
    for item in items {
        match item {
            Item::Fn(value) => {
                add_symbol(symbols, value.sig.ident.to_string(), value.sig.ident.span())
            }
            Item::Struct(value) => add_symbol(symbols, value.ident.to_string(), value.ident.span()),
            Item::Type(value) => add_symbol(symbols, value.ident.to_string(), value.ident.span()),
            Item::Trait(value) => add_symbol(symbols, value.ident.to_string(), value.ident.span()),
            Item::Enum(value) => {
                let owner = value.ident.to_string();
                add_symbol(symbols, owner.clone(), value.ident.span());
                for variant in &value.variants {
                    add_symbol(
                        symbols,
                        format!("{owner}::{}", variant.ident),
                        variant.ident.span(),
                    );
                }
            }
            Item::Impl(value) => {
                let owner = match value.self_ty.as_ref() {
                    syn::Type::Path(path) => path
                        .path
                        .segments
                        .last()
                        .map(|segment| segment.ident.to_string()),
                    _ => None,
                };
                for member in &value.items {
                    if let ImplItem::Fn(method) = member {
                        let name = method.sig.ident.to_string();
                        add_symbol(symbols, name.clone(), method.sig.ident.span());
                        if let Some(owner) = &owner {
                            add_symbol(
                                symbols,
                                format!("{owner}::{name}"),
                                method.sig.ident.span(),
                            );
                        }
                    }
                }
            }
            Item::Mod(value) => {
                if let Some((_, nested)) = &value.content {
                    collect_symbols(nested, symbols);
                }
            }
            _ => {}
        }
    }
}

fn source_path(root: &Path, relative: &str) -> io::Result<PathBuf> {
    let root = root.canonicalize()?;
    let path = root
        .join(relative)
        .canonicalize()
        .map_err(|err| invalid(format!("{relative}: {err}")))?;
    if !path.starts_with(&root) || !path.is_file() {
        return Err(invalid(format!(
            "{relative}: expected a file inside {}",
            root.display()
        )));
    }
    Ok(path)
}

fn write_jsonl<T: Serialize>(path: &Path, values: impl IntoIterator<Item = T>) -> io::Result<()> {
    let mut file = fs::File::create(path)?;
    for value in values {
        serde_json::to_writer(&mut file, &value)?;
        writeln!(file)?;
    }
    Ok(())
}

pub fn index(root: &Path, map_path: &Path, out: &Path) -> io::Result<DevStats> {
    let root = root.canonicalize()?;
    let map_source = fs::read(map_path)?;
    let map: FeatureMap = serde_json::from_slice(&map_source)?;
    if map.schema_version != SCHEMA_VERSION {
        return Err(invalid(format!(
            "feature map schema {} is unsupported; expected {SCHEMA_VERSION}",
            map.schema_version
        )));
    }
    let mut nodes = BTreeMap::<String, DevNode>::new();
    let mut edges = Vec::new();
    let mut sources = BTreeMap::new();
    let map_relative = map_path
        .canonicalize()?
        .strip_prefix(&root)
        .map_err(|_| invalid("feature map must be inside the repository"))?
        .to_string_lossy()
        .to_string();
    let mut source_cache = BTreeMap::new();
    source_cache.insert(map_relative.clone(), map_source.clone());
    sources.insert(map_relative, digest(&map_source));
    if map
        .features
        .iter()
        .any(|feature| !feature.covers.is_empty())
    {
        let path = source_path(&root, "src/ast.rs")?;
        let bytes = fs::read(path)?;
        let source = std::str::from_utf8(&bytes).map_err(|err| invalid(err.to_string()))?;
        validate_coverage(&map, &ast_variants(source)?)?;
        sources.insert("src/ast.rs".into(), digest(&bytes));
        source_cache.insert("src/ast.rs".into(), bytes);
    }
    let mut names = BTreeSet::new();
    let mut symbol_cache = BTreeMap::new();
    for feature in &map.features {
        if !names.insert(feature.name.clone()) {
            return Err(invalid(format!("duplicate feature: {}", feature.name)));
        }
        let feature_id = format!("feature:{}", feature.name);
        nodes.insert(
            feature_id.clone(),
            DevNode {
                id: feature_id.clone(),
                kind: "feature".into(),
                name: feature.name.clone(),
                aliases: feature.aliases.clone(),
                covers: feature.covers.clone(),
                path: String::new(),
                line: 0,
            },
        );
        for link in feature.links.iter().chain(&map.common_links) {
            let path = source_path(&root, &link.path)?;
            if !source_cache.contains_key(&link.path) {
                source_cache.insert(link.path.clone(), fs::read(&path)?);
            }
            let bytes = &source_cache[&link.path];
            sources.insert(link.path.clone(), digest(bytes));
            let line = if let Some(symbol) = &link.symbol {
                if !symbol_cache.contains_key(&link.path) {
                    let source =
                        std::str::from_utf8(bytes).map_err(|err| invalid(err.to_string()))?;
                    let syntax = syn::parse_file(source).map_err(|err| {
                        invalid(format!("{}: Rust parse failed: {err}", path.display()))
                    })?;
                    let mut symbols = BTreeMap::new();
                    collect_symbols(&syntax.items, &mut symbols);
                    symbol_cache.insert(link.path.clone(), symbols);
                }
                symbol_line(&path, symbol, &symbol_cache[&link.path])?
            } else {
                1
            };
            let (kind, name) = if let Some(symbol) = &link.symbol {
                ("rust_symbol", symbol.clone())
            } else {
                (link.role.as_str(), link.path.clone())
            };
            let target = format!("{kind}:{}:{name}", link.path);
            nodes.entry(target.clone()).or_insert(DevNode {
                id: target.clone(),
                kind: kind.into(),
                name,
                aliases: Vec::new(),
                covers: Vec::new(),
                path: link.path.clone(),
                line,
            });
            edges.push(DevEdge {
                from: feature_id.clone(),
                to: target,
                role: link.role.clone(),
                provenance: "curated".into(),
            });
        }
    }
    edges.sort_by(|a, b| (&a.from, &a.role, &a.to).cmp(&(&b.from, &b.role, &b.to)));
    edges.dedup_by(|a, b| a.from == b.from && a.to == b.to && a.role == b.role);
    fs::create_dir_all(out)?;
    write_jsonl(&out.join("nodes.jsonl"), nodes.values())?;
    write_jsonl(&out.join("edges.jsonl"), edges.iter())?;
    let manifest = Manifest {
        schema_version: SCHEMA_VERSION,
        generator: generator_id(),
        source_revision: revision(&root),
        sources,
    };
    fs::write(
        out.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(DevStats {
        features: names.len(),
        nodes: nodes.len(),
        edges: edges.len(),
    })
}

fn read_jsonl<T: for<'a> Deserialize<'a>>(path: &Path) -> io::Result<Vec<T>> {
    let input = io::BufReader::new(fs::File::open(path)?);
    input
        .lines()
        .map(|line| {
            let line = line?;
            serde_json::from_str(&line).map_err(io::Error::from)
        })
        .collect()
}

fn terms(query: &str) -> Vec<String> {
    let words: Vec<_> = query
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != ':')
        .flat_map(|token| {
            token
                .split(':')
                .filter(|word| !word.is_empty())
                .map(move |word| (word.to_lowercase(), token.contains("::")))
        })
        .collect();
    words
        .iter()
        .filter(|(word, qualified)| {
            *qualified
                || words.len() == 1
                || ![
                    "the", "a", "for", "to", "in", "of", "how", "where", "change",
                ]
                .contains(&word.as_str())
        })
        .map(|(word, _)| word.clone())
        .collect()
}

fn has_query_anchor(
    words: &[String],
    feature: &DevNode,
    related: &[&DevEdge],
    by_id: &BTreeMap<&str, &DevNode>,
) -> bool {
    // Feature names start with the language identifier, followed by optional
    // qualifiers (e.g. "pipe_reg output-port reset"). Roles and paths are not anchors.
    let feature_terms = terms(&feature.name);
    if feature_terms
        .first()
        .is_some_and(|name| words.contains(name))
    {
        return true;
    }
    if feature
        .aliases
        .iter()
        .chain(&feature.covers)
        .any(|alias| has_named_terms(words, alias))
    {
        return true;
    }
    related
        .iter()
        .filter_map(|edge| by_id.get(edge.to.as_str()))
        .any(|node| node.kind == "rust_symbol" && has_named_terms(words, &node.name))
}

fn has_named_terms(words: &[String], name: &str) -> bool {
    let name_terms = terms(name);
    !name_terms.is_empty()
        && words
            .windows(name_terms.len())
            .any(|window| window == name_terms)
}

pub fn query(
    root: &Path,
    index_dir: &Path,
    query: &str,
    limit: usize,
    token_budget: usize,
    roles: &[String],
    include_source: bool,
) -> io::Result<String> {
    let manifest: Manifest = serde_json::from_slice(&fs::read(index_dir.join("manifest.json"))?)?;
    if manifest.schema_version != SCHEMA_VERSION || manifest.generator != generator_id() {
        return Err(invalid(
            "developer graph schema or generator changed; rebuild with arch graph dev-index",
        ));
    }
    for (relative, expected) in &manifest.sources {
        let actual = digest(&fs::read(source_path(root, relative)?)?);
        if &actual != expected {
            return Err(invalid(format!(
                "{relative} changed since indexing; rebuild with arch graph dev-index"
            )));
        }
    }
    let nodes: Vec<DevNode> = read_jsonl(&index_dir.join("nodes.jsonl"))?;
    let edges: Vec<DevEdge> = read_jsonl(&index_dir.join("edges.jsonl"))?;
    if let Some(role) = roles
        .iter()
        .find(|role| !edges.iter().any(|edge| &edge.role == *role))
    {
        return Err(invalid(format!("unknown developer graph role: {role}")));
    }
    let by_id: BTreeMap<_, _> = nodes.iter().map(|node| (node.id.as_str(), node)).collect();
    let words = terms(query);
    if words.is_empty() {
        return Err(invalid("developer graph query requires a search term"));
    }
    let lowered_query = query.to_lowercase();
    let mut hits = Vec::new();
    for feature in nodes.iter().filter(|node| node.kind == "feature") {
        let mut related: Vec<_> = edges
            .iter()
            .filter(|edge| edge.from == feature.id)
            .collect();
        if !has_query_anchor(&words, feature, &related, &by_id) {
            continue;
        }
        related.sort_by_key(|edge| {
            let named_matches = by_id.get(edge.to.as_str()).map_or(0, |node| {
                node.name
                    .split(|ch: char| !ch.is_ascii_alphanumeric())
                    .filter(|word| !word.is_empty() && words.contains(&word.to_lowercase()))
                    .count()
            });
            (
                role_priority(&edge.role),
                std::cmp::Reverse(named_matches),
                edge.to.as_str(),
            )
        });
        let hay = format!(
            "{} {} {} {}",
            feature.name,
            feature.aliases.join(" "),
            feature.covers.join(" "),
            related
                .iter()
                .filter_map(|edge| by_id
                    .get(edge.to.as_str())
                    .map(|node| format!("{} {} {}", edge.role, node.name, node.path)))
                .collect::<Vec<_>>()
                .join(" ")
        )
        .to_lowercase();
        let matched = words
            .iter()
            .filter(|word| hay.contains(word.as_str()))
            .count();
        if matched == 0 {
            continue;
        }
        let feature_name = feature.name.to_lowercase();
        let feature_terms = terms(&feature.name);
        let exact_feature_mention = (!feature_terms.is_empty()
            && words
                .windows(feature_terms.len())
                .any(|window| window == feature_terms.as_slice()))
            || feature
                .aliases
                .iter()
                .any(|alias| has_named_terms(&words, alias));
        let exact_coverage_mention = feature
            .covers
            .iter()
            .any(|covered| has_named_terms(&words, covered));
        let score = matched * 10
            + usize::from(feature_name.contains(&lowered_query)) * 100
            + usize::from(exact_feature_mention) * 100
            + usize::from(exact_coverage_mention) * 200
            + usize::from(matched == words.len()) * 30;
        related.retain(|edge| roles.is_empty() || roles.iter().any(|role| role == &edge.role));
        if related.is_empty() {
            continue;
        }
        hits.push((score, matched, feature, related));
    }
    if words.len() > 1 && hits.iter().any(|hit| hit.1 == words.len()) {
        hits.retain(|hit| hit.1 == words.len());
    }
    hits.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.2.name.cmp(&b.2.name)));
    let total_lines: usize = hits
        .iter()
        .map(|hit| {
            1 + hit
                .3
                .iter()
                .filter(|edge| by_id.contains_key(edge.to.as_str()))
                .count()
        })
        .sum();
    let mut lines = Vec::new();
    let mut source_lines: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let mut remaining = limit.max(1);
    for (_, _, feature, related) in hits {
        if remaining == 0 {
            break;
        }
        lines.push(format!("feature {}", feature.name));
        remaining -= 1;
        for edge in related {
            if remaining == 0 {
                break;
            }
            if let Some(node) = by_id.get(edge.to.as_str()) {
                let mut line = format!(
                    "  {} {}:{} {} [{}]",
                    edge.role, node.path, node.line, node.name, edge.provenance
                );
                if include_source && node.kind == "rust_symbol" {
                    if node.line == 0 {
                        return Err(invalid(format!("{} has no source line", node.path)));
                    }
                    if !source_lines.contains_key(node.path.as_str()) {
                        source_lines.insert(
                            node.path.as_str(),
                            fs::read_to_string(source_path(root, &node.path)?)?
                                .lines()
                                .map(str::to_string)
                                .collect(),
                        );
                    }
                    let excerpt = source_lines[node.path.as_str()]
                        .get(node.line.saturating_sub(1))
                        .ok_or_else(|| {
                            invalid(format!("{}:{} is unavailable", node.path, node.line))
                        })?;
                    let excerpt = excerpt.split_whitespace().collect::<Vec<_>>().join(" ");
                    let excerpt: String = excerpt.chars().take(120).collect();
                    line.push_str(&format!(" | {excerpt}"));
                }
                lines.push(line);
                remaining -= 1;
            }
        }
    }
    if lines.is_empty() {
        return Ok(bounded_output(
            &["(no matches)".into()],
            false,
            token_budget,
        ));
    }
    Ok(bounded_output(
        &lines,
        lines.len() < total_lines,
        token_budget,
    ))
}

fn bounded_output(lines: &[String], mut truncated: bool, token_budget: usize) -> String {
    // Reserve the CLI's trailing newline. Keep whole rows, including UTF-8 source.
    let max_bytes = token_budget.max(1).saturating_mul(4).saturating_sub(1);
    let mut output = String::new();
    for line in lines {
        let extra = line.len() + usize::from(!output.is_empty());
        if output.len().saturating_add(extra) > max_bytes {
            truncated = true;
            break;
        }
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(line);
    }
    if truncated {
        let marker = if max_bytes >= "(truncated)".len() {
            "(truncated)"
        } else {
            "..."
        };
        while !output.is_empty() && output.len() + 1 + marker.len() > max_bytes {
            output.truncate(output.rfind('\n').unwrap_or(0));
        }
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(marker);
    }
    output
}

fn role_priority(role: &str) -> usize {
    match role {
        "ast" => 0,
        "parser" => 1,
        "backend" => 2,
        "semantic_check" => 3,
        "lowerer" => 4,
        "ir" => 5,
        "fixture" | "dut" => 6,
        "spec" => 7,
        "skill" => 8,
        "mcp_guidance" => 9,
        _ => 10,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COVERED_ENUMS: &[&str] = &[
        "Item",
        "ModuleBodyItem",
        "GenItem",
        "Stmt",
        "ThreadStmt",
        "FunctionBodyItem",
        "TypeExpr",
        "ExprKind",
        "BinOp",
        "UnaryOp",
    ];

    #[test]
    fn qualified_variant_names_preserve_language_keywords() {
        assert_eq!(
            terms("Find Stmt::For in source"),
            ["find", "stmt", "for", "source"]
        );
        assert_eq!(terms("for"), ["for"]);
        assert_eq!(terms("for ARCH counter"), ["arch", "counter"]);
        assert!(has_named_terms(&terms("Locate Stmt::For"), "Stmt::For"));
        assert!(!has_named_terms(&terms("Locate Stmt"), "Stmt::For"));
    }

    fn complete_coverage(map: &FeatureMap, source: &str, required: &[&str]) -> io::Result<()> {
        let variants = ast_variants(source)?;
        validate_coverage(map, &variants)?;
        let mut expected = BTreeSet::new();
        for name in required {
            expected.extend(
                variants
                    .get(*name)
                    .ok_or_else(|| invalid(format!("missing AST inventory enum {name}")))?
                    .iter()
                    .cloned(),
            );
        }
        let covered: BTreeSet<_> = map
            .features
            .iter()
            .flat_map(|feature| &feature.covers)
            .cloned()
            .collect();
        if covered != expected {
            return Err(invalid(format!(
                "AST coverage drift: missing {:?}, unexpected {:?}",
                expected.difference(&covered).collect::<Vec<_>>(),
                covered.difference(&expected).collect::<Vec<_>>()
            )));
        }
        Ok(())
    }

    #[test]
    fn bundled_map_covers_every_core_ast_variant() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let map: FeatureMap =
            serde_json::from_slice(&fs::read(root.join("doc/codegraph_feature_map.json")).unwrap())
                .unwrap();
        complete_coverage(
            &map,
            &fs::read_to_string(root.join("src/ast.rs")).unwrap(),
            COVERED_ENUMS,
        )
        .unwrap();
        for feature in &map.features {
            if !feature.covers.is_empty() {
                assert!(
                    feature.links.iter().any(|link| link.role == "ast"
                        && link.path == "src/ast.rs"
                        && link.symbol.is_some()),
                    "{} needs a source-bound AST owner",
                    feature.name
                );
            }
        }
    }

    #[test]
    fn coverage_drift_controls_reject_added_removed_renamed_and_duplicate_variants() {
        let data =
            r#"{"schema_version":1,"features":[{"name":"old","covers":["Item::Old"],"links":[]}]}"#;
        let map: FeatureMap = serde_json::from_str(data).unwrap();
        complete_coverage(&map, "enum Item { Old }", &["Item"]).unwrap();
        for source in [
            "enum Item { Old, New }",
            "enum Item {}",
            "enum Item { Renamed }",
            "enum Other { Old }",
            "enum Item { Old } enum Item { New }",
            "enum Item { Old, Old }",
        ] {
            assert!(
                complete_coverage(&map, source, &["Item"]).is_err(),
                "{source}"
            );
        }
        for data in [
            r#"{"schema_version":1,"features":[{"name":"old","covers":[],"links":[]}]}"#,
            r#"{"schema_version":1,"features":[{"name":"old","covers":["Item::Old","Item::Old"],"links":[]}]}"#,
            r#"{"schema_version":1,"features":[{"name":"old","covers":["Item::Old"],"links":[]},{"name":"other","covers":["Item::Old"],"links":[]}]}"#,
        ] {
            let map = serde_json::from_str(data).unwrap();
            assert!(
                complete_coverage(&map, "enum Item { Old }", &["Item"]).is_err(),
                "{data}"
            );
        }
    }

    #[test]
    fn custom_partial_coverage_is_validated_and_pinned_without_requiring_completeness() {
        let root = std::env::temp_dir().join(format!("arch-dev-coverage-{}", std::process::id()));
        fs::create_dir_all(root.join("src")).unwrap();
        let ast = root.join("src/ast.rs");
        fs::write(&ast, "enum Feature { Present, Other }\n").unwrap();
        let map_path = root.join("map.json");
        let out = root.join("index");
        let data = r#"{"schema_version":1,"features":[{"name":"sample","covers":["Feature::Present"],"links":[{"role":"ast","path":"src/ast.rs","symbol":"Feature::Present"}]}]}"#;
        fs::write(&map_path, data).unwrap();
        index(&root, &map_path, &out).unwrap();
        let result = query(&root, &out, "Feature::Present", 10, 100, &[], true).unwrap();
        assert!(result.contains("feature sample"));
        assert!(result.contains("Feature::Present [curated]"));
        assert_eq!(
            query(&root, &out, "sample", 10, 100, &["backend".into()], false)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
        for coverage in ["Feature::Absent", "Feature::Present\",\"Feature::Present"] {
            fs::write(&map_path, data.replacen("Feature::Present", coverage, 1)).unwrap();
            assert!(index(&root, &map_path, &out).is_err());
        }
        fs::write(root.join("owners.rs"), "fn parse_feature() {}\n").unwrap();
        let without_ast_link = r#"{"schema_version":1,"features":[{"name":"sample","covers":["Feature::Present"],"links":[{"role":"parser","path":"owners.rs","symbol":"parse_feature"}]}]}"#;
        fs::write(&map_path, without_ast_link).unwrap();
        index(&root, &map_path, &out).unwrap();
        let covered = query(&root, &out, "Feature::Present", 10, 100, &[], false).unwrap();
        assert!(covered.contains("feature sample"));
        assert!(covered.contains("parse_feature"));
        fs::write(&ast, "enum Feature { Present, Other, New }\n").unwrap();
        assert!(query(&root, &out, "sample", 10, 100, &[], false)
            .unwrap_err()
            .to_string()
            .contains("changed since indexing"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cached_symbols_keep_ambiguity_and_missing_role_checks() {
        let root =
            std::env::temp_dir().join(format!("arch-dev-cached-symbols-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("owners.rs"), "struct Alpha; struct Beta;\nimpl Alpha { fn shared() {} }\nimpl Beta { fn shared() {} }\n").unwrap();
        let map_path = root.join("map.json");
        let out = root.join("index");
        let data = r#"{"schema_version":1,"features":[{"name":"alpha","links":[{"role":"ast","path":"owners.rs","symbol":"Alpha"},{"role":"parser","path":"owners.rs","symbol":"Alpha::shared"}]},{"name":"beta","links":[{"role":"backend","path":"owners.rs","symbol":"Beta::shared"}]}]}"#;
        fs::write(&map_path, data).unwrap();
        index(&root, &map_path, &out).unwrap();
        assert_eq!(
            query(&root, &out, "alpha", 20, 200, &["backend".into()], false).unwrap(),
            "(no matches)"
        );
        assert!(query(&root, &out, "Beta::shared", 20, 200, &[], true)
            .unwrap()
            .contains("owners.rs:3"));
        for (symbol, error) in [("shared", "ambiguous"), ("Absent", "was not found")] {
            fs::write(&map_path, data.replace("Alpha::shared", symbol)).unwrap();
            assert!(index(&root, &map_path, &out)
                .unwrap_err()
                .to_string()
                .contains(error));
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn bounded_output_marks_omissions_without_splitting_rows() {
        let lines = vec!["abcd".into(), "xy".into(), "lambda: \u{03bb}".into()];
        let complete = lines.join("\n");
        for budget in 1..=40 {
            for already_truncated in [false, true] {
                let result = bounded_output(&lines, already_truncated, budget);
                assert!(result.len() + 1 <= budget * 4, "{budget}: {result}");
                if already_truncated || complete.len() + 1 > budget * 4 {
                    assert!(result.ends_with("(truncated)") || result == "...");
                    for line in result
                        .lines()
                        .filter(|line| !["(truncated)", "..."].contains(line))
                    {
                        assert!(lines.iter().any(|original| original == line));
                    }
                } else {
                    assert_eq!(result, complete);
                }
            }
        }
        assert_eq!(bounded_output(&lines[..2], false, 2), "abcd\nxy");
        assert_eq!(bounded_output(&lines, true, 5), "abcd\nxy\n(truncated)");
        assert_eq!(bounded_output(&["(no matches)".into()], false, 1), "...");
    }

    #[test]
    fn bundled_feature_map_resolves_all_symbols() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let out = std::env::temp_dir().join(format!("arch-dev-graph-test-{}", std::process::id()));
        let stats = index(root, &root.join("doc/codegraph_feature_map.json"), &out).unwrap();
        assert!(stats.features >= 8);
        let map: FeatureMap =
            serde_json::from_slice(&fs::read(root.join("doc/codegraph_feature_map.json")).unwrap())
                .unwrap();
        for feature in &map.features {
            for variant in &feature.covers {
                let result = query(root, &out, variant, 20, 200, &[], true).unwrap();
                assert_eq!(
                    result.lines().next(),
                    Some(format!("feature {}", feature.name).as_str()),
                    "{variant}: {result}"
                );
                assert!(result.contains("[curated] |"), "{variant}: {result}");
                assert!(result.len() < 800, "{variant}: {result}");
            }
        }
        for feature in ["counter", "register-file"] {
            let question = format!(
                "For ARCH {feature} declarations, locate the AST declaration type, parser function and SystemVerilog emitter."
            );
            let result = query(root, &out, &question, 20, 200, &[], true).unwrap();
            let roles: Vec<_> = result
                .lines()
                .skip(1)
                .take(3)
                .filter_map(|line| line.split_whitespace().next())
                .collect();
            assert_eq!(roles, ["ast", "parser", "backend"], "{result}");
        }
        let arbiter_question = "For ARCH round-robin arbitration, identify the AST type, parser function, and SystemVerilog backend function with their source paths.";
        for (include_source, budget) in [(false, 100), (true, 200)] {
            let result = query(
                root,
                &out,
                arbiter_question,
                20,
                budget,
                &[],
                include_source,
            )
            .unwrap();
            let first_roles: Vec<_> = result.lines().take(4).collect();
            assert_eq!(first_roles[0], "feature arbiter", "{result}");
            for (line, symbol) in first_roles[1..].iter().zip([
                "ArbiterDecl",
                "parse_arbiter",
                "emit_arbiter_round_robin",
            ]) {
                assert!(line.contains(symbol), "{result}");
            }
            assert_eq!(first_roles.len(), 4, "{result}");
        }
        let result = query(root, &out, "pipe_reg declaration", 30, 600, &[], false).unwrap();
        assert!(result.contains("emit_pipe_reg"));
        assert!(result.contains("pipe_reg_test.arch"));
        let sourced = query(root, &out, "pipe_reg declaration", 30, 800, &[], true).unwrap();
        assert!(sourced.contains("| fn emit_pipe_reg("));
        let port = query(root, &out, "pipe_reg output-port reset", 30, 800, &[], true).unwrap();
        assert!(port.contains("PortRegInfo"));
        assert!(port.contains("parse_port_decl"));
        assert!(port.contains("emit_reg_block"));
        assert!(port.contains("Prob047_dff8ar.arch"));
        assert!(!sourced.contains(" | **ARCH**"));
        assert!(!sourced.contains(" | ---"));
        let pipeline_backend = query(root, &out, "pipeline backend", 20, 200, &[], true).unwrap();
        assert!(
            pipeline_backend.contains("emit_pipeline [curated] | pub(crate) fn emit_pipeline("),
            "{pipeline_backend}"
        );
        assert!(
            !pipeline_backend.contains("emit_pipeline_inst"),
            "{pipeline_backend}"
        );
        let parser_only = query(
            root,
            &out,
            "pipeline parser",
            20,
            100,
            &["parser".into()],
            false,
        )
        .unwrap();
        assert!(parser_only.contains("parse_pipeline"));
        assert!(!parser_only.contains("parse_fifo"));
        assert!(!parser_only.contains("emit_pipeline_inst"));
        assert!(!parser_only.contains("mcp_guidance"));
        let limited = query(root, &out, "pipeline", 1, 800, &[], true).unwrap();
        assert_eq!(limited, "feature pipeline\n(truncated)");
        let complete_parser =
            query(root, &out, "pipeline", 2, 800, &["parser".into()], true).unwrap();
        assert!(!complete_parser.contains("(truncated)"));
        let tight = query(root, &out, "pipe_reg output-port reset", 30, 100, &[], true).unwrap();
        assert!(tight.ends_with("(truncated)"));
        assert!(tight.len() + 1 <= 400);
        for unsupported in [
            "For ARCH missing_construct_zz, identify its AST type, parser owner, SystemVerilog emitter, and shipped fixture with source paths.",
            "missing_construct_zz AST parser backend fixture",
            "AST parser backend fixture src/parser.rs src/ast.rs",
            "pipelines parser",
        ] {
            assert_eq!(query(root, &out, unsupported, 30, 200, &[], true).unwrap(),
                       "(no matches)", "{unsupported}");
        }
        for (query_text, expected) in [
            ("PIPE_REG", "feature pipe_reg"),
            ("arbitration", "feature arbiter"),
            ("round-robin arbitration AST parser", "feature arbiter"),
            ("Where is parse_fifo defined?", "feature fifo"),
            (
                "Which feature owns PortRegInfo?",
                "feature pipe_reg output-port reset",
            ),
            ("Find TypeChecker::check", "feature module"),
        ] {
            assert!(
                query(root, &out, query_text, 30, 200, &[], false)
                    .unwrap()
                    .contains(expected),
                "{query_text}"
            );
        }
        assert!(query(
            root,
            &out,
            "pipeline",
            20,
            100,
            &["not_a_role".into()],
            false
        )
        .is_err());
        let manifest_path = out.join("manifest.json");
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest["generator"] = "older compiler".into();
        fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert!(query(root, &out, "pipeline", 20, 100, &[], false)
            .unwrap_err()
            .to_string()
            .contains("generator changed"));
        let _ = fs::remove_dir_all(out);
    }

    #[test]
    fn stale_or_missing_mapped_symbol_is_rejected() {
        let root =
            std::env::temp_dir().join(format!("arch-dev-graph-stale-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("feature.rs");
        let map = root.join("map.json");
        fs::write(&source, "pub struct Feature;\n").unwrap();
        fs::write(&map, r#"{"schema_version":1,"features":[{"name":"feature","links":[{"role":"ast","path":"feature.rs","symbol":"Feature"}]}]}"#).unwrap();
        let out = root.join("index");
        index(&root, &map, &out).unwrap();
        assert!(query(&root, &out, "feature", 10, 100, &[], false)
            .unwrap()
            .contains("feature.rs:1"));
        fs::write(&source, "pub struct Renamed;\n").unwrap();
        assert!(query(&root, &out, "feature", 10, 100, &[], false)
            .unwrap_err()
            .to_string()
            .contains("rebuild"));
        assert!(index(&root, &map, &out)
            .unwrap_err()
            .to_string()
            .contains("was not found"));
        let _ = fs::remove_dir_all(root);
    }
}
