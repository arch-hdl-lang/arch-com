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

fn symbol_line(path: &Path, symbol: &str) -> io::Result<usize> {
    let source = fs::read_to_string(path)?;
    let syntax = syn::parse_file(&source)
        .map_err(|err| invalid(format!("{}: Rust parse failed: {err}", path.display())))?;
    let mut symbols = BTreeMap::<String, Vec<usize>>::new();
    collect_symbols(&syntax.items, &mut symbols);
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
    sources.insert(map_relative, digest(&map_source));
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
                path: String::new(),
                line: 0,
            },
        );
        for link in feature.links.iter().chain(&map.common_links) {
            let path = source_path(&root, &link.path)?;
            let bytes = fs::read(&path)?;
            sources.insert(link.path.clone(), digest(&bytes));
            let line = if let Some(symbol) = &link.symbol {
                let key = (link.path.clone(), symbol.clone());
                if let Some(line) = symbol_cache.get(&key) {
                    *line
                } else {
                    let line = symbol_line(&path, symbol)?;
                    symbol_cache.insert(key, line);
                    line
                }
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
    query
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .filter(|word| {
            ![
                "the", "a", "for", "to", "in", "of", "how", "where", "change",
            ]
            .contains(&word.as_str())
        })
        .collect()
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
        related.sort_by_key(|edge| (role_priority(&edge.role), edge.to.as_str()));
        let hay = format!(
            "{} {}",
            feature.name,
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
        let exact_feature_mention = !feature_terms.is_empty()
            && words
                .windows(feature_terms.len())
                .any(|window| window == feature_terms.as_slice());
        let score = matched * 10
            + usize::from(feature_name.contains(&lowered_query)) * 100
            + usize::from(exact_feature_mention) * 100
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
        return Ok("(no matches)".into());
    }
    let max_chars = token_budget.max(1).saturating_mul(4);
    let mut output = String::new();
    for line in lines {
        let extra = line.len() + usize::from(!output.is_empty());
        if output.len() + extra > max_chars {
            break;
        }
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&line);
    }
    Ok(output)
}

fn role_priority(role: &str) -> usize {
    match role {
        "ast" => 0,
        "parser" => 1,
        "semantic_check" => 2,
        "lowerer" => 3,
        "ir" => 4,
        "backend" => 5,
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

    #[test]
    fn bundled_feature_map_resolves_all_symbols() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let out = std::env::temp_dir().join(format!("arch-dev-graph-test-{}", std::process::id()));
        let stats = index(root, &root.join("doc/codegraph_feature_map.json"), &out).unwrap();
        assert!(stats.features >= 8);
        let result = query(root, &out, "pipe_reg reset", 30, 600, &[], false).unwrap();
        assert!(result.contains("emit_pipe_reg"));
        assert!(result.contains("pipe_reg_test.arch"));
        let sourced = query(root, &out, "pipe_reg reset", 30, 800, &[], true).unwrap();
        assert!(sourced.contains("| fn emit_pipe_reg("));
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
