//! Compiler-native code graph indexing for ARCH projects.
//!
//! V1 is intentionally structural/syntactic: it emits deterministic JSONL
//! records from compiler-owned AST facts and labels edge confidence instead of
//! pretending to have complete data/control-flow analysis.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ast::*;
use crate::lexer::Span;

#[derive(Debug, Clone)]
pub struct SourceSegment {
    pub start: usize,
    pub end: usize,
    pub filename: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanInfo {
    pub byte_start: usize,
    pub byte_end: usize,
    pub line_start: usize,
    pub col_start: usize,
    pub line_end: usize,
    pub col_end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRecord {
    pub id: String,
    pub rel_path: String,
    pub abs_path: String,
    pub sha256: String,
    pub mtime: u64,
    pub root_input: bool,
    pub parse_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeRecord {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<SpanInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attrs: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeRecord {
    pub src: String,
    pub dst: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<SpanInfo>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attrs: BTreeMap<String, String>,
    pub confidence: String,
}

#[derive(Debug, Clone)]
pub struct GraphIndex {
    pub index_root: String,
    pub files: Vec<FileRecord>,
    pub nodes: Vec<NodeRecord>,
    pub edges: Vec<EdgeRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestRecord {
    pub schema_version: u32,
    pub generator: String,
    #[serde(default)]
    pub index_root: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryHit {
    pub score: i64,
    pub node: NodeRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactHit {
    pub depth: usize,
    pub via: String,
    pub node: NodeRecord,
}

pub fn build_index(
    ast: &SourceFile,
    segments: &[SourceSegment],
    root_inputs: &BTreeSet<String>,
    index_root: &Path,
) -> std::io::Result<GraphIndex> {
    let source_map = SourceMap::new(segments, index_root);
    let mut builder = Builder::new(source_map, root_inputs)?;
    builder.add_file_records();

    for item in &ast.items {
        builder.add_construct(item);
    }
    for item in &ast.items {
        builder.add_construct_edges(item);
    }

    builder.finish()
}

pub fn merge_indexes(indexes: Vec<GraphIndex>) -> GraphIndex {
    let mut index_root: Option<String> = None;
    let mut roots_match = true;
    let mut files = BTreeMap::new();
    let mut nodes = BTreeMap::new();
    let mut edge_keys = BTreeSet::new();
    let mut edges = Vec::new();
    for index in indexes {
        if let Some(existing) = &index_root {
            if existing != &index.index_root {
                roots_match = false;
            }
        } else {
            index_root = Some(index.index_root.clone());
        }
        for file in index.files {
            files.entry(file.id.clone()).or_insert(file);
        }
        for node in index.nodes {
            nodes.entry(node.id.clone()).or_insert(node);
        }
        for edge in index.edges {
            let key = serde_json::to_string(&edge).unwrap_or_default();
            if edge_keys.insert(key) {
                edges.push(edge);
            }
        }
    }
    let mut files: Vec<_> = files.into_values().collect();
    let mut nodes: Vec<_> = nodes.into_values().collect();
    files.sort_by(|a, b| a.id.cmp(&b.id));
    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    edges.sort_by(|a, b| {
        a.src
            .cmp(&b.src)
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.dst.cmp(&b.dst))
            .then_with(|| span_info_start(&a.span).cmp(&span_info_start(&b.span)))
    });
    GraphIndex {
        index_root: if roots_match {
            index_root.unwrap_or_default()
        } else {
            String::new()
        },
        files,
        nodes,
        edges,
    }
}

pub fn write_index(index: &GraphIndex, out_dir: &Path, clean: bool) -> std::io::Result<()> {
    if out_dir.exists() && !clean {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!(
                "{} already exists; pass --clean to replace it",
                out_dir.display()
            ),
        ));
    }

    let parent = out_dir.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let tmp_name = format!(
        ".{}.tmp-{}",
        out_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("archgraph"),
        std::process::id()
    );
    let tmp_dir = parent.join(tmp_name);
    if tmp_dir.exists() {
        fs::remove_dir_all(&tmp_dir)?;
    }
    fs::create_dir_all(&tmp_dir)?;

    write_jsonl(&tmp_dir.join("files.jsonl"), &index.files)?;
    write_jsonl(&tmp_dir.join("nodes.jsonl"), &index.nodes)?;
    write_jsonl(&tmp_dir.join("edges.jsonl"), &index.edges)?;
    let manifest = ManifestRecord {
        schema_version: 1,
        generator: format!("arch {}", env!("CARGO_PKG_VERSION")),
        index_root: index.index_root.clone(),
        files: vec![
            "files.jsonl".to_string(),
            "nodes.jsonl".to_string(),
            "edges.jsonl".to_string(),
        ],
    };
    fs::write(
        tmp_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;

    if out_dir.exists() {
        let backup_dir = parent.join(format!(
            ".{}.bak-{}",
            out_dir
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("archgraph"),
            std::process::id()
        ));
        if backup_dir.exists() {
            fs::remove_dir_all(&backup_dir)?;
        }
        fs::rename(out_dir, &backup_dir)?;
        if let Err(err) = fs::rename(&tmp_dir, out_dir) {
            let _ = fs::rename(&backup_dir, out_dir);
            return Err(err);
        }
        fs::remove_dir_all(backup_dir)?;
    } else {
        fs::rename(tmp_dir, out_dir)?;
    }
    Ok(())
}

pub fn load_index(index_dir: &Path) -> std::io::Result<GraphIndex> {
    let manifest_path = index_dir.join("manifest.json");
    let mut index_root = String::new();
    if manifest_path.exists() {
        let manifest: ManifestRecord = serde_json::from_str(&fs::read_to_string(&manifest_path)?)
            .map_err(|err| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{}: {err}", manifest_path.display()),
            )
        })?;
        if manifest.schema_version != 1 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "unsupported graph schema version {}",
                    manifest.schema_version
                ),
            ));
        }
        index_root = manifest.index_root;
    }
    Ok(GraphIndex {
        index_root,
        files: read_jsonl_with_path(&index_dir.join("files.jsonl"))?,
        nodes: read_jsonl_with_path(&index_dir.join("nodes.jsonl"))?,
        edges: read_jsonl_with_path(&index_dir.join("edges.jsonl"))?,
    })
}

pub fn query(index: &GraphIndex, query: &str, limit: usize) -> Vec<QueryHit> {
    if query.trim().is_empty() || limit == 0 {
        return Vec::new();
    }
    let q = query.to_ascii_lowercase();
    let terms = tokenize(query);
    let mut hits: Vec<QueryHit> = index
        .nodes
        .iter()
        .filter_map(|node| {
            let mut score = 0_i64;
            let name = node.name.to_ascii_lowercase();
            let kind = node.kind.to_ascii_lowercase();
            let file = node.file.to_ascii_lowercase();
            let doc = node.doc.as_deref().unwrap_or("").to_ascii_lowercase();
            let scope = node.scope.as_deref().unwrap_or("").to_ascii_lowercase();
            let qualified = if scope.is_empty() {
                name.clone()
            } else {
                format!("{scope}.{name}")
            };

            if qualified == q {
                score += 260;
            } else if name == q {
                score += 200;
            } else if qualified.contains(&q) {
                score += 130;
            } else if name.contains(&q) {
                score += 100;
            }
            if kind == q {
                score += 60;
            }
            if file.contains(&q) {
                score += 40;
            }
            if doc.contains(&q) {
                score += 30;
            }
            let matched = score > 0;
            if matched && is_construct_like(&node.kind) {
                score += 20;
            } else if matched && matches!(node.kind.as_str(), "generated_sv" | "interface_stub") {
                score -= 15;
            }
            for term in &terms {
                if name.contains(term) {
                    score += 35;
                }
                if scope.contains(term) {
                    score += 18;
                }
                if doc.contains(term) {
                    score += 10;
                }
                if file.contains(term) {
                    score += 8;
                }
            }
            (score > 0).then(|| QueryHit {
                score,
                node: node.clone(),
            })
        })
        .collect();
    hits.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.node.file.cmp(&b.node.file))
            .then_with(|| span_start(&a.node).cmp(&span_start(&b.node)))
            .then_with(|| a.node.id.cmp(&b.node.id))
    });
    hits.truncate(limit);
    hits
}

pub fn callers(index: &GraphIndex, target: &str, limit: usize) -> Vec<(EdgeRecord, NodeRecord)> {
    if target.trim().is_empty() || limit == 0 {
        return Vec::new();
    }
    let node_by_id = node_map(index);
    let target_ids: BTreeSet<String> = index
        .nodes
        .iter()
        .filter(|n| {
            n.name.eq_ignore_ascii_case(target)
                || n.attrs
                    .get("qualified_name")
                    .map(|q| q.eq_ignore_ascii_case(target))
                    .unwrap_or(false)
        })
        .map(|n| n.id.clone())
        .collect();
    let mut out = Vec::new();
    for edge in &index.edges {
        if edge.kind != "calls" {
            continue;
        }
        let dst_match = target_ids.contains(&edge.dst)
            || edge
                .attrs
                .get("callee")
                .or_else(|| edge.attrs.get("method"))
                .map(|name| name.eq_ignore_ascii_case(target))
                .unwrap_or(false)
            || edge
                .attrs
                .get("qualified_name")
                .map(|name| name.eq_ignore_ascii_case(target))
                .unwrap_or(false);
        if dst_match {
            if let Some(src_node) = node_by_id.get(&edge.src) {
                out.push((edge.clone(), (*src_node).clone()));
            }
        }
    }
    out.sort_by(|a, b| {
        a.1.file
            .cmp(&b.1.file)
            .then_with(|| span_start(&a.1).cmp(&span_start(&b.1)))
            .then_with(|| a.1.id.cmp(&b.1.id))
    });
    out.truncate(limit);
    out
}

pub fn impact(index: &GraphIndex, symbol: &str, depth: usize, limit: usize) -> Vec<ImpactHit> {
    if symbol.trim().is_empty() || limit == 0 {
        return Vec::new();
    }
    let mut starts = query(index, symbol, 8);
    if starts.is_empty() {
        return Vec::new();
    }
    let best_score = starts[0].score;
    starts.retain(|hit| hit.score == best_score);
    let node_by_id = node_map(index);
    let mut adjacency: BTreeMap<String, Vec<(&EdgeRecord, String)>> = BTreeMap::new();
    for edge in &index.edges {
        if !impact_edge_kind(&edge.kind) {
            continue;
        }
        adjacency
            .entry(edge.src.clone())
            .or_default()
            .push((edge, edge.dst.clone()));
    }

    let mut seen = BTreeSet::new();
    let mut queue = VecDeque::new();
    for hit in starts {
        seen.insert(hit.node.id.clone());
        queue.push_back((hit.node.id, 0_usize, String::from("start")));
    }

    let mut out = Vec::new();
    while let Some((id, d, via)) = queue.pop_front() {
        if d > 0 {
            if let Some(node) = node_by_id.get(&id) {
                if out.len() >= limit {
                    break;
                }
                out.push(ImpactHit {
                    depth: d,
                    via: via.clone(),
                    node: (*node).clone(),
                });
            }
        }
        if d >= depth {
            continue;
        }
        if let Some(nexts) = adjacency.get(&id) {
            for (edge, next) in nexts {
                if seen.insert(next.clone()) {
                    queue.push_back((next.clone(), d + 1, edge.kind.clone()));
                }
            }
        }
    }
    out
}

pub fn context(index: &GraphIndex, task: &str, limit: usize) -> Vec<QueryHit> {
    let mut direct = query(index, task, limit.max(8));
    let node_by_id = node_map(index);
    let mut ids: BTreeSet<String> = direct.iter().map(|h| h.node.id.clone()).collect();
    let mut extras = Vec::new();
    for hit in &direct {
        for edge in index
            .edges
            .iter()
            .filter(|e| e.src == hit.node.id || e.dst == hit.node.id)
        {
            let other = if edge.src == hit.node.id {
                &edge.dst
            } else {
                &edge.src
            };
            if ids.insert(other.clone()) {
                if let Some(node) = node_by_id.get(other) {
                    extras.push(QueryHit {
                        score: hit.score.saturating_sub(25).max(1),
                        node: (*node).clone(),
                    });
                }
            }
        }
    }
    direct.extend(extras);
    direct.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.node.file.cmp(&b.node.file))
            .then_with(|| span_start(&a.node).cmp(&span_start(&b.node)))
    });
    direct.truncate(limit);
    direct
}

pub fn format_query_hits(hits: &[QueryHit]) -> String {
    if hits.is_empty() {
        return "No graph matches.".to_string();
    }
    hits.iter()
        .map(|hit| {
            format!(
                "{}  {}  {}  score={}",
                location_label(&hit.node),
                hit.node.kind,
                hit.node.name,
                hit.score
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn format_callers(hits: &[(EdgeRecord, NodeRecord)]) -> String {
    if hits.is_empty() {
        return "No indexed call edges matched.".to_string();
    }
    hits.iter()
        .map(|(edge, node)| {
            let callee = edge
                .attrs
                .get("callee")
                .or_else(|| edge.attrs.get("method"))
                .cloned()
                .unwrap_or_else(|| edge.dst.clone());
            format!(
                "{}  {} {} -> {}",
                edge_location_label(edge, node),
                node.kind,
                node.name,
                callee
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn format_impact(hits: &[ImpactHit]) -> String {
    if hits.is_empty() {
        return "No graph impact found.".to_string();
    }
    hits.iter()
        .map(|hit| {
            format!(
                "depth={} via={}  {}  {} {}",
                hit.depth,
                hit.via,
                location_label(&hit.node),
                hit.node.kind,
                hit.node.name
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn render_html(index: &GraphIndex, title: &str) -> std::io::Result<String> {
    let data = serde_json::json!({
        "indexRoot": &index.index_root,
        "files": &index.files,
        "nodes": &index.nodes,
        "edges": &index.edges,
    });
    let data = escape_json_for_script(&serde_json::to_string(&data)?);
    let title_html = html_escape(title);
    Ok(format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title_html}</title>
<style>
:root {{
  color-scheme: light;
  --bg: #f7f8fa;
  --panel: #ffffff;
  --line: #d9dee7;
  --text: #1d2430;
  --muted: #687386;
  --accent: #1267a8;
  --accent-soft: #e8f2fb;
  --edge: #4b5565;
  --shadow: 0 1px 2px rgba(16, 24, 40, 0.08);
}}
* {{ box-sizing: border-box; }}
body {{
  margin: 0;
  min-height: 100vh;
  background: var(--bg);
  color: var(--text);
  font: 14px/1.45 ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
}}
header {{
  display: flex;
  align-items: center;
  gap: 18px;
  padding: 14px 18px;
  border-bottom: 1px solid var(--line);
  background: var(--panel);
}}
h1 {{
  margin: 0;
  font-size: 18px;
  font-weight: 650;
}}
.meta {{
  color: var(--muted);
  font-size: 12px;
}}
.shell {{
  display: grid;
  grid-template-columns: minmax(300px, 390px) minmax(460px, 1fr);
  gap: 0;
  height: calc(100vh - 57px);
}}
.sidebar {{
  border-right: 1px solid var(--line);
  background: var(--panel);
  overflow: hidden;
  display: flex;
  flex-direction: column;
}}
.tools {{
  padding: 12px;
  border-bottom: 1px solid var(--line);
  display: grid;
  gap: 8px;
}}
input, select {{
  width: 100%;
  min-height: 34px;
  border: 1px solid var(--line);
  border-radius: 6px;
  padding: 6px 9px;
  color: var(--text);
  background: #fff;
  font: inherit;
}}
.counts {{
  display: flex;
  gap: 10px;
  color: var(--muted);
  font-size: 12px;
}}
.node-list {{
  overflow: auto;
  padding: 8px;
}}
.node-row {{
  width: 100%;
  text-align: left;
  border: 1px solid transparent;
  border-radius: 6px;
  padding: 8px;
  background: transparent;
  cursor: pointer;
  display: grid;
  gap: 2px;
}}
.node-row:hover, .node-row.active {{
  background: var(--accent-soft);
  border-color: #b9d8ef;
}}
.kind {{
  display: inline-flex;
  align-items: center;
  width: fit-content;
  border: 1px solid var(--line);
  border-radius: 999px;
  padding: 1px 7px;
  color: var(--muted);
  font-size: 11px;
  background: #fff;
}}
.node-name {{
  font-weight: 640;
  overflow-wrap: anywhere;
}}
.loc {{
  color: var(--muted);
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 12px;
  overflow-wrap: anywhere;
}}
main {{
  overflow: auto;
  padding: 18px;
}}
.detail {{
  max-width: 1180px;
  display: grid;
  gap: 14px;
}}
.panel {{
  background: var(--panel);
  border: 1px solid var(--line);
  border-radius: 8px;
  box-shadow: var(--shadow);
  padding: 14px;
}}
.detail-head {{
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
}}
.detail h2 {{
  margin: 4px 0 2px;
  font-size: 24px;
  line-height: 1.2;
  overflow-wrap: anywhere;
}}
.link {{
  color: var(--accent);
  text-decoration: none;
}}
.link:hover {{ text-decoration: underline; }}
.grid {{
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  gap: 10px;
}}
.kv {{
  border: 1px solid var(--line);
  border-radius: 6px;
  padding: 9px;
  background: #fbfcfe;
}}
.kv b {{
  display: block;
  margin-bottom: 3px;
  color: var(--muted);
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0;
}}
pre {{
  margin: 0;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 12px;
}}
.edges {{
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
  gap: 14px;
}}
.focus-graph {{
  display: grid;
  grid-template-columns: minmax(160px, 1fr) 90px minmax(180px, 1fr) 90px minmax(160px, 1fr);
  gap: 10px;
  align-items: center;
}}
.graph-col {{
  display: grid;
  gap: 8px;
}}
.graph-col-title {{
  color: var(--muted);
  font-size: 11px;
  font-weight: 650;
  text-transform: uppercase;
  letter-spacing: 0;
}}
.graph-node {{
  border: 1px solid var(--line);
  border-radius: 6px;
  padding: 8px;
  background: #fbfcfe;
  color: var(--text);
  cursor: pointer;
  text-align: left;
  font: inherit;
  overflow-wrap: anywhere;
}}
.graph-node:hover, .graph-node.center {{
  border-color: #9ac7e8;
  background: var(--accent-soft);
}}
.graph-arrow {{
  color: var(--muted);
  text-align: center;
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
}}
.edge-list {{
  display: grid;
  gap: 8px;
}}
.edge-card {{
  border: 1px solid var(--line);
  border-radius: 6px;
  padding: 9px;
  background: #fbfcfe;
}}
.edge-kind {{
  color: var(--edge);
  font-weight: 650;
  font-size: 12px;
}}
.edge-target {{
  display: block;
  margin-top: 3px;
  border: 0;
  padding: 0;
  background: transparent;
  color: var(--accent);
  cursor: pointer;
  font: inherit;
  text-align: left;
  overflow-wrap: anywhere;
}}
.doc {{
  border-left: 3px solid var(--accent);
  padding-left: 10px;
  color: #2f3a4b;
}}
.empty {{
  color: var(--muted);
  padding: 18px;
}}
@media (max-width: 860px) {{
  .shell {{ grid-template-columns: 1fr; height: auto; }}
  .sidebar {{ border-right: 0; border-bottom: 1px solid var(--line); max-height: 55vh; }}
  main {{ padding: 12px; }}
  .focus-graph {{ grid-template-columns: 1fr; }}
  .graph-arrow {{ display: none; }}
}}
</style>
</head>
<body>
<header>
  <h1>{title_html}</h1>
  <div class="meta" id="summary"></div>
</header>
<div class="shell arch-graph-viewer">
  <aside class="sidebar">
    <div class="tools">
      <input id="search" type="search" placeholder="Search names, kinds, files, docs">
      <select id="kind"></select>
      <div class="counts" id="counts"></div>
    </div>
    <div class="node-list" id="nodeList"></div>
  </aside>
  <main>
    <div class="detail" id="detail"></div>
  </main>
</div>
<script>
const graph = {data};
const files = new Map(graph.files.map(f => [f.rel_path, f]));
const nodes = new Map(graph.nodes.map(n => [n.id, n]));
const outgoing = new Map();
const incoming = new Map();
for (const edge of graph.edges) {{
  if (!outgoing.has(edge.src)) outgoing.set(edge.src, []);
  if (!incoming.has(edge.dst)) incoming.set(edge.dst, []);
  outgoing.get(edge.src).push(edge);
  incoming.get(edge.dst).push(edge);
}}
const listEl = document.getElementById('nodeList');
const detailEl = document.getElementById('detail');
const searchEl = document.getElementById('search');
const kindEl = document.getElementById('kind');
const countsEl = document.getElementById('counts');
const summaryEl = document.getElementById('summary');
let selectedId = graph.nodes[0]?.id || null;
summaryEl.textContent = `${{graph.nodes.length}} nodes, ${{graph.edges.length}} edges${{graph.indexRoot ? `, root ${{graph.indexRoot}}` : ''}}`;

function esc(value) {{
  return String(value ?? '').replace(/[&<>"']/g, ch => ({{'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}}[ch]));
}}
function label(node) {{
  if (!node) return 'unresolved';
  return node.scope ? `${{node.scope}}.${{node.name}}` : node.name;
}}
function loc(node) {{
  if (!node) return '';
  return node.span ? `${{node.file}}:${{node.span.line_start}}` : node.file;
}}
function fileHref(node) {{
  const file = files.get(node.file);
  if (!file || !file.abs_path) return '';
  let href = 'file://' + file.abs_path;
  if (node.span) href += '#L' + node.span.line_start;
  return href;
}}
function edgeLoc(edge, fallbackNode) {{
  if (edge.span && fallbackNode) return `${{fallbackNode.file}}:${{edge.span.line_start}}`;
  return fallbackNode ? loc(fallbackNode) : '';
}}
function searchText(node) {{
  return [node.kind, node.name, node.scope || '', node.file, node.doc || '', Object.values(node.attrs || {{}}).join(' ')].join(' ').toLowerCase();
}}
function populateKinds() {{
  const kinds = [...new Set(graph.nodes.map(n => n.kind))].sort();
  kindEl.innerHTML = '<option value="">All node kinds</option>' + kinds.map(k => `<option value="${{esc(k)}}">${{esc(k)}}</option>`).join('');
}}
function filteredNodes() {{
  const q = searchEl.value.trim().toLowerCase();
  const terms = q ? q.split(/\s+/) : [];
  const kind = kindEl.value;
  return graph.nodes.filter(node => {{
    if (kind && node.kind !== kind) return false;
    const text = searchText(node);
    return terms.every(term => text.includes(term));
  }});
}}
function renderList() {{
  const rows = filteredNodes();
  countsEl.textContent = `${{rows.length}} shown`;
  listEl.innerHTML = rows.map(node => `
    <button class="node-row${{node.id === selectedId ? ' active' : ''}}" data-node-id="${{esc(node.id)}}">
      <span class="kind">${{esc(node.kind)}}</span>
      <span class="node-name">${{esc(label(node))}}</span>
      <span class="loc">${{esc(loc(node))}}</span>
    </button>
  `).join('') || '<div class="empty">No matching nodes.</div>';
}}
function attrsHtml(node) {{
  const attrs = Object.entries(node.attrs || {{}});
  if (!attrs.length) return '';
  return `<div class="kv"><b>Attributes</b><pre>${{esc(attrs.map(([k, v]) => `${{k}}: ${{v}}`).join('\n'))}}</pre></div>`;
}}
function edgeCards(edges, mode) {{
  if (!edges.length) return '<div class="empty">No edges.</div>';
  return edges.map(edge => {{
    const otherId = mode === 'out' ? edge.dst : edge.src;
    const other = nodes.get(otherId);
    const source = nodes.get(edge.src);
    const where = edgeLoc(edge, source);
    const attrs = Object.entries(edge.attrs || {{}}).map(([k, v]) => `${{k}}=${{v}}`).join(' ');
    return `<div class="edge-card">
      <div class="edge-kind">${{esc(edge.kind)}} <span class="meta">${{esc(edge.confidence)}}${{attrs ? ' · ' + esc(attrs) : ''}}</span></div>
      <button class="edge-target" data-node-id="${{esc(otherId)}}">${{esc(other ? `${{other.kind}} ${{label(other)}}` : otherId)}}</button>
      <div class="loc">${{esc(where)}}</div>
    </div>`;
  }}).join('');
}}
function graphNodeButton(node, extraClass = '') {{
  if (!node) return '';
  return `<button class="graph-node ${{extraClass}}" data-node-id="${{esc(node.id)}}">
    <span class="kind">${{esc(node.kind)}}</span>
    <div>${{esc(label(node))}}</div>
    <div class="loc">${{esc(loc(node))}}</div>
  </button>`;
}}
function neighborColumn(edges, mode) {{
  const seen = new Set();
  const buttons = [];
  for (const edge of edges) {{
    const id = mode === 'in' ? edge.src : edge.dst;
    if (seen.has(id)) continue;
    seen.add(id);
    const node = nodes.get(id);
    if (node) buttons.push(graphNodeButton(node));
    if (buttons.length >= 8) break;
  }}
  return buttons.join('') || '<div class="empty">No linked nodes.</div>';
}}
function neighborhoodHtml(node, inEdges, outEdges) {{
  return `<section class="panel">
    <h3>Neighborhood</h3>
    <div class="focus-graph">
      <div class="graph-col"><div class="graph-col-title">Incoming</div>${{neighborColumn(inEdges, 'in')}}</div>
      <div class="graph-arrow">-&gt;</div>
      <div class="graph-col"><div class="graph-col-title">Selected</div>${{graphNodeButton(node, 'center')}}</div>
      <div class="graph-arrow">-&gt;</div>
      <div class="graph-col"><div class="graph-col-title">Outgoing</div>${{neighborColumn(outEdges, 'out')}}</div>
    </div>
  </section>`;
}}
function renderDetail(id) {{
  const node = nodes.get(id);
  if (!node) {{
    detailEl.innerHTML = '<div class="panel empty">Select a node.</div>';
    return;
  }}
  selectedId = id;
  const href = fileHref(node);
  const outEdges = outgoing.get(id) || [];
  const inEdges = incoming.get(id) || [];
  detailEl.innerHTML = `
    <section class="panel">
      <div class="detail-head">
        <div>
          <span class="kind">${{esc(node.kind)}}</span>
          <h2>${{esc(label(node))}}</h2>
          <div class="loc">${{esc(node.id)}}</div>
        </div>
        ${{href ? `<a class="link" href="${{esc(href)}}">Open source</a>` : ''}}
      </div>
    </section>
    <section class="grid">
      <div class="kv"><b>Location</b><div>${{esc(loc(node))}}</div></div>
      <div class="kv"><b>File</b><div>${{esc(node.file)}}</div></div>
      <div class="kv"><b>Scope</b><div>${{esc(node.scope || 'top level')}}</div></div>
      ${{attrsHtml(node)}}
    </section>
    ${{node.doc ? `<section class="panel doc"><pre>${{esc(node.doc)}}</pre></section>` : ''}}
    ${{neighborhoodHtml(node, inEdges, outEdges)}}
    <section class="edges">
      <div class="panel"><h3>Outgoing</h3><div class="edge-list">${{edgeCards(outEdges, 'out')}}</div></div>
      <div class="panel"><h3>Incoming</h3><div class="edge-list">${{edgeCards(inEdges, 'in')}}</div></div>
    </section>
  `;
  renderList();
}}
listEl.addEventListener('click', event => {{
  const target = event.target.closest('[data-node-id]');
  if (target) renderDetail(target.dataset.nodeId);
}});
detailEl.addEventListener('click', event => {{
  const target = event.target.closest('[data-node-id]');
  if (target) renderDetail(target.dataset.nodeId);
}});
searchEl.addEventListener('input', renderList);
kindEl.addEventListener('change', renderList);
populateKinds();
renderList();
renderDetail(selectedId);
</script>
</body>
</html>
"#
    ))
}

fn write_jsonl<T: Serialize>(path: &Path, records: &[T]) -> std::io::Result<()> {
    let mut file = fs::File::create(path)?;
    for record in records {
        serde_json::to_writer(&mut file, record)?;
        file.write_all(b"\n")?;
    }
    Ok(())
}

fn read_jsonl<T: for<'de> Deserialize<'de>>(path: &Path) -> std::io::Result<Vec<T>> {
    let file = fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    let mut out = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        out.push(serde_json::from_str(&line)?);
    }
    Ok(out)
}

fn read_jsonl_with_path<T: for<'de> Deserialize<'de>>(path: &Path) -> std::io::Result<Vec<T>> {
    read_jsonl(path)
        .map_err(|err| std::io::Error::new(err.kind(), format!("{}: {}", path.display(), err)))
}

fn escape_json_for_script(value: &str) -> String {
    value
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\'', "\\u0027")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn impact_edge_kind(kind: &str) -> bool {
    matches!(
        kind,
        "calls" | "instantiates" | "connects" | "uses_type" | "reads" | "writes" | "drives"
    )
}

fn is_construct_like(kind: &str) -> bool {
    matches!(
        kind,
        "module"
            | "fsm"
            | "fifo"
            | "ram"
            | "cam"
            | "counter"
            | "arbiter"
            | "regfile"
            | "pipeline"
            | "linklist"
            | "bus"
            | "package"
            | "function"
            | "struct"
            | "enum"
            | "domain"
    )
}

fn is_builtin_method(name: &str) -> bool {
    matches!(
        name,
        "trunc" | "zext" | "sext" | "reverse" | "as_uint" | "as_sint"
    )
}

struct Builder {
    source_map: SourceMap,
    root_inputs: BTreeSet<String>,
    files: Vec<FileRecord>,
    nodes: BTreeMap<String, NodeRecord>,
    edges: Vec<EdgeRecord>,
    construct_by_name: BTreeMap<String, String>,
    symbol_by_scope: BTreeMap<String, String>,
    bus_port_by_scope: BTreeMap<String, String>,
}

impl Builder {
    fn new(source_map: SourceMap, root_inputs: &BTreeSet<String>) -> std::io::Result<Self> {
        Ok(Self {
            source_map,
            root_inputs: root_inputs.clone(),
            files: Vec::new(),
            nodes: BTreeMap::new(),
            edges: Vec::new(),
            construct_by_name: BTreeMap::new(),
            symbol_by_scope: BTreeMap::new(),
            bus_port_by_scope: BTreeMap::new(),
        })
    }

    fn add_file_records(&mut self) {
        let segments: Vec<_> = self.source_map.segments.clone();
        for seg in &segments {
            let rel = self.source_map.rel_path(&seg.filename);
            let abs = abs_path(&seg.filename);
            let id = file_id(&rel);
            let sha256 = hex_sha256(seg.source.as_bytes());
            let mtime = fs::metadata(&seg.filename)
                .and_then(|md| md.modified())
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let root_input = self.root_inputs.contains(&seg.filename)
                || self.root_inputs.contains(&abs)
                || self.root_inputs.contains(&rel);
            self.files.push(FileRecord {
                id: id.clone(),
                rel_path: rel.clone(),
                abs_path: abs,
                sha256,
                mtime,
                root_input,
                parse_status: "ok".to_string(),
            });
            self.add_node(NodeRecord {
                id,
                kind: "file".to_string(),
                name: rel.clone(),
                file: rel.clone(),
                span: None,
                scope: None,
                doc: file_inner_doc(&seg.source),
                attrs: BTreeMap::new(),
            });
        }
    }

    fn add_construct(&mut self, item: &Item) {
        let construct = item.as_construct();
        let kind = construct.kind_label().replace(' ', "_");
        let name = construct.name().name.clone();
        let rel = self.rel_for_span(construct.span());
        let id = construct_id(&rel, &kind, &name);
        self.construct_by_name
            .entry(name.clone())
            .or_insert(id.clone());
        let doc = join_docs(construct.doc(), construct.inner_doc());
        self.add_node(NodeRecord {
            id: id.clone(),
            kind: kind.clone(),
            name: name.clone(),
            file: rel.clone(),
            span: self.span_info(construct.span()),
            scope: None,
            doc,
            attrs: BTreeMap::new(),
        });
        self.symbol_by_scope
            .entry(symbol_key(&rel, "", &name))
            .or_insert(id.clone());
        self.add_edge(
            file_id(&rel),
            id.clone(),
            "defines",
            construct.span(),
            "exact",
        );

        if let Some(doc) = construct.doc() {
            self.add_doc_node(&id, &rel, &name, "outer", doc, construct.span());
        }
        if let Some(doc) = construct.inner_doc() {
            self.add_doc_node(&id, &rel, &name, "inner", doc, construct.span());
        }

        self.add_artifacts(item, &id, &rel, &name, construct.span());
        self.add_params_ports(item, &id, &rel, &name);
        self.add_body_nodes(item, &id, &rel, &name);
    }

    fn add_construct_edges(&mut self, item: &Item) {
        match item {
            Item::Use(u) => self.add_import(u),
            Item::Module(m) => self.add_module_edges(m),
            Item::Fsm(f) => self.add_fsm_edges(f),
            Item::Function(f) => {
                let rel = self.rel_for_span(f.span);
                let owner = construct_id(&rel, "function", &f.name.name);
                self.walk_function_body(&owner, &rel, &f.name.name, &f.body);
                self.walk_type(&owner, &f.name.name, &f.ret_ty, f.span);
                for arg in &f.args {
                    self.walk_type(&owner, &f.name.name, &arg.ty, arg.name.span);
                }
            }
            Item::Bus(b) => {
                let rel = self.rel_for_span(b.span);
                let owner = construct_id(&rel, "bus", &b.name.name);
                for p in &b.params {
                    if let ParamKind::Type(ty) = &p.kind {
                        self.walk_type(&owner, &b.name.name, ty, p.span);
                    }
                }
                for s in &b.signals {
                    self.walk_type(&owner, &b.name.name, &s.ty, s.span);
                }
                for m in &b.tlm_methods {
                    let id = method_id(&rel, &b.name.name, &m.name.name);
                    let mut attrs = BTreeMap::new();
                    attrs.insert(
                        "qualified_name".to_string(),
                        format!("{}.{}", b.name.name, m.name.name),
                    );
                    attrs.insert("mode".to_string(), m.mode.name.clone());
                    self.add_node(NodeRecord {
                        id: id.clone(),
                        kind: "function".to_string(),
                        name: m.name.name.clone(),
                        file: rel.clone(),
                        span: self.span_info(m.span),
                        scope: Some(b.name.name.clone()),
                        doc: None,
                        attrs,
                    });
                    self.symbol_by_scope
                        .entry(symbol_key(&rel, &b.name.name, &m.name.name))
                        .or_insert(id.clone());
                    self.construct_by_name
                        .entry(format!("{}.{}", b.name.name, m.name.name))
                        .or_insert(id.clone());
                    self.add_edge(owner.clone(), id, "contains", m.span, "exact");
                    for (_, ty) in &m.args {
                        self.walk_type(&owner, &b.name.name, ty, m.span);
                    }
                    if let Some(ty) = &m.ret {
                        self.walk_type(&owner, &b.name.name, ty, m.span);
                    }
                }
            }
            _ => {}
        }
    }

    fn finish(mut self) -> std::io::Result<GraphIndex> {
        self.files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
        self.edges.sort_by(|a, b| {
            a.src
                .cmp(&b.src)
                .then_with(|| a.kind.cmp(&b.kind))
                .then_with(|| a.dst.cmp(&b.dst))
                .then_with(|| span_info_start(&a.span).cmp(&span_info_start(&b.span)))
        });
        Ok(GraphIndex {
            index_root: normalize_path(&self.source_map.index_root),
            files: self.files,
            nodes: self.nodes.into_values().collect(),
            edges: self.edges,
        })
    }

    fn add_import(&mut self, u: &UseDecl) {
        let rel = self.rel_for_span(u.span);
        let use_id = construct_id(&rel, "use", &u.name.name);
        let dst = self
            .construct_by_name
            .get(&u.name.name)
            .cloned()
            .unwrap_or_else(|| unresolved_id(&u.name.name));
        self.add_edge(use_id, dst, "imports", u.span, "exact");
    }

    fn add_module_edges(&mut self, m: &ModuleDecl) {
        let rel = self.rel_for_span(m.span);
        let owner = construct_id(&rel, "module", &m.name.name);
        for p in &m.ports {
            self.walk_type(&owner, &m.name.name, &p.ty, p.span);
            if let Some(bus) = &p.bus_info {
                self.add_uses_type(&owner, &bus.bus_name.name, p.span);
            }
        }
        for b in &m.body {
            match b {
                ModuleBodyItem::RegDecl(r) => {
                    self.walk_type(&owner, &m.name.name, &r.ty, r.span);
                    if let Some(e) = &r.init {
                        self.walk_expr(&owner, &rel, &m.name.name, e, ExprUse::Read);
                    }
                }
                ModuleBodyItem::WireDecl(w) => self.walk_type(&owner, &m.name.name, &w.ty, w.span),
                ModuleBodyItem::PipeRegDecl(p) => {
                    let src = signal_id(&rel, &m.name.name, &p.source.name);
                    let dst = signal_id(&rel, &m.name.name, &p.name.name);
                    self.add_edge(src, dst, "drives", p.span, "syntactic");
                }
                ModuleBodyItem::LetBinding(l) => {
                    self.walk_expr(&owner, &rel, &m.name.name, &l.value, ExprUse::Read);
                    if let Some(ty) = &l.ty {
                        self.walk_type(&owner, &m.name.name, ty, l.span);
                    }
                    if l.ty.is_some() {
                        let dst = signal_id(&rel, &m.name.name, &l.name.name);
                        self.add_edge(owner.clone(), dst, "writes", l.span, "syntactic");
                    }
                }
                ModuleBodyItem::CombBlock(c) => {
                    self.walk_stmts(&owner, &rel, &m.name.name, &c.stmts);
                }
                ModuleBodyItem::RegBlock(r) => {
                    self.walk_stmts(&owner, &rel, &m.name.name, &r.stmts);
                }
                ModuleBodyItem::LatchBlock(l) => {
                    self.walk_stmts(&owner, &rel, &m.name.name, &l.stmts)
                }
                ModuleBodyItem::Inst(inst) => self.add_inst_edges(&owner, &rel, &m.name.name, inst),
                ModuleBodyItem::Generate(g) => self.walk_generate(&owner, &rel, &m.name.name, g),
                ModuleBodyItem::Thread(t) => self.walk_thread(&owner, &rel, &m.name.name, t),
                ModuleBodyItem::Assert(a) => {
                    self.walk_expr(&owner, &rel, &m.name.name, &a.expr, ExprUse::Read)
                }
                ModuleBodyItem::Function(f) => {
                    self.walk_function_body(&owner, &rel, &m.name.name, &f.body)
                }
                ModuleBodyItem::TlmConnect(c) => {
                    self.add_tlm_connect_edges(&owner, &rel, &m.name.name, c)
                }
                ModuleBodyItem::TypeAlias(t) => self.walk_type(&owner, &m.name.name, &t.ty, t.span),
                ModuleBodyItem::Resource(_) => {}
            }
        }
    }

    fn add_fsm_edges(&mut self, f: &FsmDecl) {
        let rel = self.rel_for_span(f.span);
        let owner = construct_id(&rel, "fsm", &f.name.name);
        for p in &f.ports {
            self.walk_type(&owner, &f.name.name, &p.ty, p.span);
        }
        for r in &f.regs {
            self.walk_type(&owner, &f.name.name, &r.ty, r.span);
        }
        for l in &f.lets {
            self.walk_expr(&owner, &rel, &f.name.name, &l.value, ExprUse::Read);
        }
        self.walk_stmts(&owner, &rel, &f.name.name, &f.default_comb);
        self.walk_stmts(&owner, &rel, &f.name.name, &f.default_seq);
        for state in &f.states {
            self.walk_stmts(&owner, &rel, &f.name.name, &state.comb_stmts);
            self.walk_stmts(&owner, &rel, &f.name.name, &state.seq_stmts);
            for tr in &state.transitions {
                self.walk_expr(&owner, &rel, &f.name.name, &tr.condition, ExprUse::Read);
            }
        }
    }

    fn add_inst_edges(&mut self, owner: &str, rel: &str, scope: &str, inst: &InstDecl) {
        let inst_id = instance_id(rel, scope, &inst.name.name);
        let target = self
            .construct_by_name
            .get(&inst.module_name.name)
            .cloned()
            .unwrap_or_else(|| unresolved_id(&inst.module_name.name));
        self.add_edge(inst_id.clone(), target, "instantiates", inst.span, "exact");
        for pa in &inst.param_assigns {
            self.walk_expr(owner, rel, scope, &pa.value, ExprUse::Read);
            if let Some(ty) = &pa.ty {
                self.walk_type(owner, scope, ty, pa.name.span);
            }
        }
        for conn in &inst.connections {
            let signal = expr_signal_name(&conn.signal).unwrap_or_else(|| expr_label(&conn.signal));
            let signal_node_id = expr_signal_name(&conn.signal)
                .map(|name| self.resolve_symbol(rel, scope, &name))
                .unwrap_or_else(|| signal_id(rel, scope, &signal.replace('.', "_")));
            let mut attrs = BTreeMap::new();
            attrs.insert("child_port".to_string(), conn.port_name.name.clone());
            attrs.insert(
                "direction".to_string(),
                match conn.direction {
                    ConnectDir::Input => "input".to_string(),
                    ConnectDir::Output => "output".to_string(),
                },
            );
            self.add_edge_with_attrs(
                inst_id.clone(),
                signal_node_id.clone(),
                "connects",
                conn.span,
                "syntactic",
                attrs,
            );
            match conn.direction {
                ConnectDir::Input => {
                    self.add_edge(
                        signal_node_id,
                        inst_id.clone(),
                        "reads",
                        conn.span,
                        "syntactic",
                    );
                }
                ConnectDir::Output => {
                    self.add_edge(
                        inst_id.clone(),
                        signal_node_id,
                        "drives",
                        conn.span,
                        "syntactic",
                    );
                }
            }
            self.walk_expr(owner, rel, scope, &conn.signal, ExprUse::Read);
        }
    }

    fn add_tlm_connect_edges(&mut self, owner: &str, rel: &str, scope: &str, c: &TlmConnectDecl) {
        for target in &c.targets {
            let mut attrs = BTreeMap::new();
            attrs.insert(
                "from".to_string(),
                format!("{}.{}", c.from_inst.name, c.from_port.name),
            );
            attrs.insert(
                "to".to_string(),
                format!("{}.{}", target.to_inst.name, target.to_port.name),
            );
            self.add_edge_with_attrs(
                owner.to_string(),
                instance_id(rel, scope, &target.to_inst.name),
                "connects",
                target.span,
                "syntactic",
                attrs,
            );
        }
    }

    fn add_artifacts(&mut self, item: &Item, owner: &str, rel: &str, name: &str, span: Span) {
        let sv_rel = replace_ext(rel, "sv");
        let sv_id = format!("artifact:{sv_rel}#sv:{name}");
        self.add_node(NodeRecord {
            id: sv_id.clone(),
            kind: "generated_sv".to_string(),
            name: format!("{name}.sv"),
            file: sv_rel.clone(),
            span: None,
            scope: Some(name.to_string()),
            doc: None,
            attrs: artifact_attrs(&sv_rel),
        });
        self.add_edge(owner.to_string(), sv_id, "generates_sv", span, "exact");

        if crate::interface::emit_interface(item).is_some() {
            let archi_rel = sibling_file(rel, &format!("{name}.archi"));
            let archi_id = format!("artifact:{archi_rel}#archi:{name}");
            self.add_node(NodeRecord {
                id: archi_id.clone(),
                kind: "interface_stub".to_string(),
                name: format!("{name}.archi"),
                file: archi_rel.clone(),
                span: None,
                scope: Some(name.to_string()),
                doc: None,
                attrs: artifact_attrs(&archi_rel),
            });
            self.add_edge(
                owner.to_string(),
                archi_id,
                "generates_interface",
                span,
                "exact",
            );
        }
    }

    fn add_params_ports(&mut self, item: &Item, owner: &str, rel: &str, scope: &str) {
        for p in item_params(item) {
            let id = param_id(rel, scope, &p.name.name);
            self.add_node(NodeRecord {
                id: id.clone(),
                kind: "param".to_string(),
                name: p.name.name.clone(),
                file: rel.to_string(),
                span: self.span_info(p.span),
                scope: Some(scope.to_string()),
                doc: None,
                attrs: BTreeMap::new(),
            });
            self.symbol_by_scope
                .entry(symbol_key(rel, scope, &p.name.name))
                .or_insert(id.clone());
            self.add_edge(owner.to_string(), id, "has_param", p.span, "exact");
            if let ParamKind::Type(ty) = &p.kind {
                self.walk_type(owner, scope, ty, p.span);
            }
        }
        for p in item_ports(item) {
            let id = port_id(rel, scope, &p.name.name);
            let mut attrs = BTreeMap::new();
            attrs.insert(
                "direction".to_string(),
                match p.direction {
                    Direction::In => "in".to_string(),
                    Direction::Out => "out".to_string(),
                },
            );
            if let Some(bus) = &p.bus_info {
                attrs.insert("bus".to_string(), bus.bus_name.name.clone());
                self.bus_port_by_scope
                    .entry(symbol_key(rel, scope, &p.name.name))
                    .or_insert(bus.bus_name.name.clone());
            }
            self.add_node(NodeRecord {
                id: id.clone(),
                kind: "port".to_string(),
                name: p.name.name.clone(),
                file: rel.to_string(),
                span: self.span_info(p.span),
                scope: Some(scope.to_string()),
                doc: None,
                attrs,
            });
            self.symbol_by_scope
                .entry(symbol_key(rel, scope, &p.name.name))
                .or_insert(id.clone());
            self.add_edge(owner.to_string(), id, "has_port", p.span, "exact");
        }
    }

    fn add_body_nodes(&mut self, item: &Item, owner: &str, rel: &str, scope: &str) {
        match item {
            Item::Module(m) => {
                for b in &m.body {
                    match b {
                        ModuleBodyItem::RegDecl(r) => {
                            self.add_signal(owner, rel, scope, &r.name.name, r.span, "reg")
                        }
                        ModuleBodyItem::WireDecl(w) => {
                            self.add_signal(owner, rel, scope, &w.name.name, w.span, "wire")
                        }
                        ModuleBodyItem::PipeRegDecl(p) => {
                            self.add_signal(owner, rel, scope, &p.name.name, p.span, "pipe_reg")
                        }
                        ModuleBodyItem::LetBinding(l) if l.ty.is_some() => {
                            self.add_signal(owner, rel, scope, &l.name.name, l.span, "let")
                        }
                        ModuleBodyItem::LetBinding(l) => {
                            self.symbol_by_scope
                                .entry(symbol_key(rel, scope, &l.name.name))
                                .or_insert_with(|| signal_id(rel, scope, &l.name.name));
                        }
                        ModuleBodyItem::Inst(i) => {
                            let id = instance_id(rel, scope, &i.name.name);
                            let mut attrs = BTreeMap::new();
                            attrs.insert("module".to_string(), i.module_name.name.clone());
                            self.add_node(NodeRecord {
                                id: id.clone(),
                                kind: "instance".to_string(),
                                name: i.name.name.clone(),
                                file: rel.to_string(),
                                span: self.span_info(i.span),
                                scope: Some(scope.to_string()),
                                doc: None,
                                attrs,
                            });
                            self.symbol_by_scope
                                .entry(symbol_key(rel, scope, &i.name.name))
                                .or_insert(id.clone());
                            self.add_edge(owner.to_string(), id, "contains", i.span, "exact");
                        }
                        ModuleBodyItem::Function(f) => {
                            let id = function_id(rel, scope, &f.name.name);
                            let mut attrs = BTreeMap::new();
                            attrs.insert(
                                "qualified_name".to_string(),
                                format!("{scope}.{}", f.name.name),
                            );
                            self.add_node(NodeRecord {
                                id: id.clone(),
                                kind: "function".to_string(),
                                name: f.name.name.clone(),
                                file: rel.to_string(),
                                span: self.span_info(f.span),
                                scope: Some(scope.to_string()),
                                doc: join_docs(f.doc.as_deref(), f.inner_doc.as_deref()),
                                attrs,
                            });
                            self.symbol_by_scope
                                .entry(symbol_key(rel, scope, &f.name.name))
                                .or_insert(id.clone());
                            self.construct_by_name
                                .entry(f.name.name.clone())
                                .or_insert(id.clone());
                            self.construct_by_name
                                .entry(format!("{scope}.{}", f.name.name))
                                .or_insert(id.clone());
                            self.add_edge(owner.to_string(), id, "contains", f.span, "exact");
                        }
                        _ => {}
                    }
                }
            }
            Item::Fsm(f) => {
                for r in &f.regs {
                    self.add_signal(owner, rel, scope, &r.name.name, r.span, "reg");
                }
                for w in &f.wires {
                    self.add_signal(owner, rel, scope, &w.name.name, w.span, "wire");
                }
                for l in &f.lets {
                    if l.ty.is_some() {
                        self.add_signal(owner, rel, scope, &l.name.name, l.span, "let");
                    }
                }
            }
            _ => {}
        }
    }

    fn add_signal(
        &mut self,
        owner: &str,
        rel: &str,
        scope: &str,
        name: &str,
        span: Span,
        flavor: &str,
    ) {
        let mut attrs = BTreeMap::new();
        attrs.insert("flavor".to_string(), flavor.to_string());
        let id = signal_id(rel, scope, name);
        self.add_node(NodeRecord {
            id: id.clone(),
            kind: "signal".to_string(),
            name: name.to_string(),
            file: rel.to_string(),
            span: self.span_info(span),
            scope: Some(scope.to_string()),
            doc: None,
            attrs,
        });
        self.symbol_by_scope
            .entry(symbol_key(rel, scope, name))
            .or_insert(id.clone());
        self.add_edge(owner.to_string(), id, "contains", span, "exact");
    }

    fn add_doc_node(
        &mut self,
        owner: &str,
        rel: &str,
        name: &str,
        flavor: &str,
        doc: &str,
        span: Span,
    ) {
        let id = format!("{owner}#doc:{flavor}");
        let mut attrs = BTreeMap::new();
        attrs.insert("flavor".to_string(), flavor.to_string());
        self.add_node(NodeRecord {
            id: id.clone(),
            kind: "doc".to_string(),
            name: format!("{name}:{flavor}"),
            file: rel.to_string(),
            span: self.span_info(span),
            scope: Some(name.to_string()),
            doc: Some(doc.to_string()),
            attrs,
        });
        self.add_edge(owner.to_string(), id, "has_doc", span, "exact");
    }

    fn walk_generate(&mut self, owner: &str, rel: &str, scope: &str, g: &GenerateDecl) {
        match g {
            GenerateDecl::For(f) => {
                self.walk_expr(owner, rel, scope, &f.start, ExprUse::Read);
                self.walk_expr(owner, rel, scope, &f.end, ExprUse::Read);
                for item in &f.items {
                    self.walk_gen_item(owner, rel, scope, item);
                }
            }
            GenerateDecl::If(i) => {
                self.walk_expr(owner, rel, scope, &i.cond, ExprUse::Read);
                for item in i.then_items.iter().chain(i.else_items.iter()) {
                    self.walk_gen_item(owner, rel, scope, item);
                }
            }
        }
    }

    fn walk_gen_item(&mut self, owner: &str, rel: &str, scope: &str, item: &GenItem) {
        match item {
            GenItem::Inst(i) => self.add_inst_edges(owner, rel, scope, i),
            GenItem::TlmConnect(c) => self.add_tlm_connect_edges(owner, rel, scope, c),
            GenItem::Thread(t) => self.walk_thread(owner, rel, scope, t),
            GenItem::Assert(a) => self.walk_expr(owner, rel, scope, &a.expr, ExprUse::Read),
            GenItem::Seq(rb) => self.walk_stmts(owner, rel, scope, &rb.stmts),
            GenItem::Comb(cb) => self.walk_stmts(owner, rel, scope, &cb.stmts),
            GenItem::Wire(w) => self.walk_type(owner, scope, &w.ty, w.span),
            GenItem::Port(p) => self.walk_type(owner, scope, &p.ty, p.span),
        }
    }

    fn walk_stmts(&mut self, owner: &str, rel: &str, scope: &str, stmts: &[Stmt]) {
        for stmt in stmts {
            match stmt {
                Stmt::Assign(a) => {
                    self.walk_expr(owner, rel, scope, &a.value, ExprUse::Read);
                    self.walk_expr(owner, rel, scope, &a.target, ExprUse::Write);
                }
                Stmt::IfElse(i) => {
                    self.walk_expr(owner, rel, scope, &i.cond, ExprUse::Read);
                    self.walk_stmts(owner, rel, scope, &i.then_stmts);
                    self.walk_stmts(owner, rel, scope, &i.else_stmts);
                }
                Stmt::Match(m) => {
                    self.walk_expr(owner, rel, scope, &m.scrutinee, ExprUse::Read);
                    for arm in &m.arms {
                        self.walk_stmts(owner, rel, scope, &arm.body);
                    }
                }
                Stmt::Log(l) => {
                    for arg in &l.args {
                        self.walk_expr(owner, rel, scope, arg, ExprUse::Read);
                    }
                }
                Stmt::For(f) => self.walk_stmts(owner, rel, scope, &f.body),
                Stmt::Init(i) => self.walk_stmts(owner, rel, scope, &i.body),
                Stmt::WaitUntil(e, _) => self.walk_expr(owner, rel, scope, e, ExprUse::Read),
                Stmt::DoUntil { body, cond, .. } => {
                    self.walk_stmts(owner, rel, scope, body);
                    self.walk_expr(owner, rel, scope, cond, ExprUse::Read);
                }
            }
        }
    }

    fn walk_thread(&mut self, owner: &str, rel: &str, scope: &str, t: &ThreadBlock) {
        if let Some((cond, body)) = &t.default_when {
            self.walk_expr(owner, rel, scope, cond, ExprUse::Read);
            self.walk_thread_stmts(owner, rel, scope, body);
        }
        self.walk_stmts(owner, rel, scope, &t.default_comb);
        self.walk_thread_stmts(owner, rel, scope, &t.body);
    }

    fn walk_thread_stmts(&mut self, owner: &str, rel: &str, scope: &str, stmts: &[ThreadStmt]) {
        for stmt in stmts {
            match stmt {
                ThreadStmt::CombAssign(a)
                | ThreadStmt::SeqAssign(a)
                | ThreadStmt::ForkTlmAssign(a) => {
                    self.walk_expr(owner, rel, scope, &a.value, ExprUse::Read);
                    self.walk_expr(owner, rel, scope, &a.target, ExprUse::Write);
                }
                ThreadStmt::WaitUntil(e, _) | ThreadStmt::WaitCycles(e, _) => {
                    self.walk_expr(owner, rel, scope, e, ExprUse::Read);
                }
                ThreadStmt::IfElse(i) => {
                    self.walk_expr(owner, rel, scope, &i.cond, ExprUse::Read);
                    self.walk_thread_stmts(owner, rel, scope, &i.then_stmts);
                    self.walk_thread_stmts(owner, rel, scope, &i.else_stmts);
                }
                ThreadStmt::ForkJoin(branches, _) => {
                    for branch in branches {
                        self.walk_thread_stmts(owner, rel, scope, branch);
                    }
                }
                ThreadStmt::For {
                    start, end, body, ..
                } => {
                    self.walk_expr(owner, rel, scope, start, ExprUse::Read);
                    self.walk_expr(owner, rel, scope, end, ExprUse::Read);
                    self.walk_thread_stmts(owner, rel, scope, body);
                }
                ThreadStmt::Lock { body, .. } => self.walk_thread_stmts(owner, rel, scope, body),
                ThreadStmt::DoUntil { body, cond, .. } => {
                    self.walk_thread_stmts(owner, rel, scope, body);
                    self.walk_expr(owner, rel, scope, cond, ExprUse::Read);
                }
                ThreadStmt::Log(l) => {
                    for arg in &l.args {
                        self.walk_expr(owner, rel, scope, arg, ExprUse::Read);
                    }
                }
                ThreadStmt::Return(e, _) => self.walk_expr(owner, rel, scope, e, ExprUse::Read),
                ThreadStmt::JoinAll(_) => {}
            }
        }
    }

    fn walk_function_body(
        &mut self,
        owner: &str,
        rel: &str,
        scope: &str,
        body: &[FunctionBodyItem],
    ) {
        for item in body {
            match item {
                FunctionBodyItem::Let(l) => {
                    self.walk_expr(owner, rel, scope, &l.value, ExprUse::Read);
                    if let Some(ty) = &l.ty {
                        self.walk_type(owner, scope, ty, l.span);
                    }
                }
                FunctionBodyItem::Return(e) => self.walk_expr(owner, rel, scope, e, ExprUse::Read),
                FunctionBodyItem::IfElse(i) => {
                    self.walk_expr(owner, rel, scope, &i.cond, ExprUse::Read);
                    self.walk_function_body(owner, rel, scope, &i.then_body);
                    self.walk_function_body(owner, rel, scope, &i.else_body);
                }
                FunctionBodyItem::For(f) => self.walk_function_body(owner, rel, scope, &f.body),
                FunctionBodyItem::Assign(a) => {
                    self.walk_expr(owner, rel, scope, &a.value, ExprUse::Read);
                    self.walk_expr(owner, rel, scope, &a.target, ExprUse::Write);
                }
            }
        }
    }

    fn walk_expr(&mut self, owner: &str, rel: &str, scope: &str, expr: &Expr, use_kind: ExprUse) {
        match &expr.kind {
            ExprKind::Ident(name) | ExprKind::SynthIdent(name, _) => {
                let edge_kind = match use_kind {
                    ExprUse::Read => "reads",
                    ExprUse::Write => "writes",
                };
                let dst = self.resolve_symbol(rel, scope, name);
                self.add_edge(owner.to_string(), dst, edge_kind, expr.span, "syntactic");
            }
            ExprKind::Binary(_, a, b) => {
                self.walk_expr(owner, rel, scope, a, ExprUse::Read);
                self.walk_expr(owner, rel, scope, b, ExprUse::Read);
            }
            ExprKind::Unary(_, e)
            | ExprKind::Cast(e, _)
            | ExprKind::LatencyAt(e, _)
            | ExprKind::Signed(e)
            | ExprKind::Unsigned(e)
            | ExprKind::Clog2(e)
            | ExprKind::Onehot(e)
            | ExprKind::SvaNext(_, e) => self.walk_expr(owner, rel, scope, e, ExprUse::Read),
            ExprKind::FieldAccess(base, _) => self.walk_expr(owner, rel, scope, base, use_kind),
            ExprKind::MethodCall(base, method, args) => {
                self.walk_expr(owner, rel, scope, base, ExprUse::Read);
                if !is_builtin_method(&method.name) {
                    let mut attrs = BTreeMap::new();
                    attrs.insert("method".to_string(), method.name.clone());
                    let receiver = receiver_label(base);
                    let bus_qualified = receiver.as_deref().and_then(|recv| {
                        self.bus_port_by_scope
                            .get(&symbol_key(rel, scope, recv))
                            .map(|bus| format!("{bus}.{}", method.name))
                    });
                    let qualified = bus_qualified
                        .or_else(|| receiver.map(|recv| format!("{recv}.{}", method.name)))
                        .unwrap_or_else(|| method.name.clone());
                    attrs.insert("qualified_name".to_string(), qualified.clone());
                    let dst = self
                        .construct_by_name
                        .get(&qualified)
                        .cloned()
                        .or_else(|| {
                            self.symbol_by_scope
                                .get(&symbol_key(rel, scope, &method.name))
                                .cloned()
                        })
                        .unwrap_or_else(|| method_id(rel, scope, &method.name));
                    self.add_edge_with_attrs(
                        owner.to_string(),
                        dst,
                        "calls",
                        method.span,
                        "syntactic",
                        attrs,
                    );
                }
                for arg in args {
                    self.walk_expr(owner, rel, scope, arg, ExprUse::Read);
                }
            }
            ExprKind::Index(base, idx) => {
                self.walk_expr(owner, rel, scope, base, use_kind);
                self.walk_expr(owner, rel, scope, idx, ExprUse::Read);
            }
            ExprKind::BitSlice(base, hi, lo) => {
                self.walk_expr(owner, rel, scope, base, use_kind);
                self.walk_expr(owner, rel, scope, hi, ExprUse::Read);
                self.walk_expr(owner, rel, scope, lo, ExprUse::Read);
            }
            ExprKind::PartSelect(base, start, width, _) => {
                self.walk_expr(owner, rel, scope, base, use_kind);
                self.walk_expr(owner, rel, scope, start, ExprUse::Read);
                self.walk_expr(owner, rel, scope, width, ExprUse::Read);
            }
            ExprKind::StructLiteral(name, fields) => {
                self.add_uses_type(owner, &name.name, name.span);
                for f in fields {
                    self.walk_expr(owner, rel, scope, &f.value, ExprUse::Read);
                }
            }
            ExprKind::FunctionCall(name, args) => {
                let mut attrs = BTreeMap::new();
                attrs.insert("callee".to_string(), name.clone());
                attrs.insert("qualified_name".to_string(), format!("{scope}.{name}"));
                let dst = self
                    .construct_by_name
                    .get(&format!("{scope}.{name}"))
                    .cloned()
                    .or_else(|| self.construct_by_name.get(name).cloned())
                    .or_else(|| {
                        self.symbol_by_scope
                            .get(&symbol_key(rel, scope, name))
                            .cloned()
                    })
                    .unwrap_or_else(|| unresolved_id(name));
                self.add_edge_with_attrs(
                    owner.to_string(),
                    dst,
                    "calls",
                    expr.span,
                    "syntactic",
                    attrs,
                );
                for arg in args {
                    self.walk_expr(owner, rel, scope, arg, ExprUse::Read);
                }
            }
            ExprKind::Ternary(c, t, e) => {
                self.walk_expr(owner, rel, scope, c, ExprUse::Read);
                self.walk_expr(owner, rel, scope, t, ExprUse::Read);
                self.walk_expr(owner, rel, scope, e, ExprUse::Read);
            }
            ExprKind::Match(scrut, arms) => {
                self.walk_expr(owner, rel, scope, scrut, ExprUse::Read);
                for arm in arms {
                    self.walk_stmts(owner, rel, scope, &arm.body);
                }
            }
            ExprKind::ExprMatch(scrut, arms) => {
                self.walk_expr(owner, rel, scope, scrut, ExprUse::Read);
                for arm in arms {
                    self.walk_expr(owner, rel, scope, &arm.value, ExprUse::Read);
                }
            }
            ExprKind::Concat(v) => {
                for e in v {
                    self.walk_expr(owner, rel, scope, e, ExprUse::Read);
                }
            }
            ExprKind::Repeat(e, n) => {
                self.walk_expr(owner, rel, scope, e, ExprUse::Read);
                self.walk_expr(owner, rel, scope, n, ExprUse::Read);
            }
            ExprKind::Inside(e, members) => {
                self.walk_expr(owner, rel, scope, e, ExprUse::Read);
                for m in members {
                    match m {
                        InsideMember::Single(e) => {
                            self.walk_expr(owner, rel, scope, e, ExprUse::Read)
                        }
                        InsideMember::Range(a, b) => {
                            self.walk_expr(owner, rel, scope, a, ExprUse::Read);
                            self.walk_expr(owner, rel, scope, b, ExprUse::Read);
                        }
                    }
                }
            }
            ExprKind::EnumVariant(name, _) => self.add_uses_type(owner, &name.name, name.span),
            ExprKind::Literal(_) | ExprKind::Todo | ExprKind::Bool(_) => {}
        }
    }

    fn walk_type(&mut self, owner: &str, scope: &str, ty: &TypeExpr, span: Span) {
        match ty {
            TypeExpr::UInt(e) | TypeExpr::SInt(e) => {
                let rel = self.rel_for_span(e.span);
                self.walk_expr(owner, &rel, scope, e, ExprUse::Read);
            }
            TypeExpr::Clock(domain) => self.add_uses_type(owner, &domain.name, domain.span),
            TypeExpr::Reset(_, _) | TypeExpr::Bool | TypeExpr::Bit => {}
            TypeExpr::Vec(inner, n) => {
                self.walk_type(owner, scope, inner, span);
                let rel = self.rel_for_span(n.span);
                self.walk_expr(owner, &rel, scope, n, ExprUse::Read);
            }
            TypeExpr::Named(name) => self.add_uses_type(owner, &name.name, name.span),
        }
    }

    fn add_uses_type(&mut self, owner: &str, name: &str, span: Span) {
        let dst = self
            .construct_by_name
            .get(name)
            .cloned()
            .unwrap_or_else(|| unresolved_id(name));
        self.add_edge(owner.to_string(), dst, "uses_type", span, "syntactic");
    }

    fn resolve_symbol(&self, rel: &str, scope: &str, name: &str) -> String {
        self.symbol_by_scope
            .get(&symbol_key(rel, scope, name))
            .cloned()
            .or_else(|| {
                self.symbol_by_scope
                    .get(&symbol_key(rel, "", name))
                    .cloned()
            })
            .or_else(|| self.construct_by_name.get(name).cloned())
            .unwrap_or_else(|| unresolved_id(name))
    }

    fn rel_for_span(&self, span: Span) -> String {
        self.source_map
            .segment_for(span.start)
            .map(|s| self.source_map.rel_path(&s.filename))
            .unwrap_or_else(|| "unknown".to_string())
    }

    fn span_info(&self, span: Span) -> Option<SpanInfo> {
        self.source_map.span_info(span)
    }

    fn add_node(&mut self, node: NodeRecord) {
        self.nodes.entry(node.id.clone()).or_insert(node);
    }

    fn add_edge(&mut self, src: String, dst: String, kind: &str, span: Span, confidence: &str) {
        self.add_edge_with_attrs(src, dst, kind, span, confidence, BTreeMap::new());
    }

    fn add_edge_with_attrs(
        &mut self,
        src: String,
        dst: String,
        kind: &str,
        span: Span,
        confidence: &str,
        attrs: BTreeMap<String, String>,
    ) {
        self.edges.push(EdgeRecord {
            src,
            dst,
            kind: kind.to_string(),
            span: self.span_info(span),
            attrs,
            confidence: confidence.to_string(),
        });
    }
}

#[derive(Debug, Clone)]
struct SourceMap {
    segments: Vec<SourceSegment>,
    index_root: PathBuf,
}

impl SourceMap {
    fn new(segments: &[SourceSegment], index_root: &Path) -> Self {
        Self {
            segments: segments.to_vec(),
            index_root: index_root
                .canonicalize()
                .unwrap_or_else(|_| index_root.to_path_buf()),
        }
    }

    fn rel_path(&self, path: &str) -> String {
        rel_path_from(&self.index_root, path)
    }

    fn segment_for(&self, offset: usize) -> Option<&SourceSegment> {
        self.segments
            .iter()
            .find(|s| offset >= s.start && offset < s.end)
            .or_else(|| self.segments.last())
    }

    fn span_info(&self, span: Span) -> Option<SpanInfo> {
        let seg = self.segment_for(span.start)?;
        let local_start = span.start.saturating_sub(seg.start);
        let local_end = span.end.min(seg.end).saturating_sub(seg.start);
        let (line_start, col_start) = line_col(&seg.source, local_start);
        let (line_end, col_end) = line_col(&seg.source, local_end);
        Some(SpanInfo {
            byte_start: local_start,
            byte_end: local_end,
            line_start,
            col_start,
            line_end,
            col_end,
        })
    }
}

#[derive(Debug, Clone, Copy)]
enum ExprUse {
    Read,
    Write,
}

fn item_params(item: &Item) -> &[ParamDecl] {
    match item {
        Item::Module(m) => &m.params,
        Item::Fsm(f) => &f.params,
        Item::Fifo(f) => &f.params,
        Item::Ram(r) => &r.params,
        Item::Cam(c) => &c.params,
        Item::Counter(c) => &c.params,
        Item::Arbiter(a) => &a.params,
        Item::Regfile(r) => &r.params,
        Item::Pipeline(p) => &p.params,
        Item::Linklist(l) => &l.params,
        Item::Bus(b) => &b.params,
        Item::Synchronizer(s) => &s.params,
        Item::Clkgate(c) => &c.params,
        Item::Template(t) => &t.params,
        Item::Package(p) => &p.params,
        _ => &[],
    }
}

fn item_ports(item: &Item) -> &[PortDecl] {
    match item {
        Item::Module(m) => &m.ports,
        Item::Fsm(f) => &f.ports,
        Item::Fifo(f) => &f.ports,
        Item::Ram(r) => &r.ports,
        Item::Cam(c) => &c.ports,
        Item::Counter(c) => &c.ports,
        Item::Arbiter(a) => &a.ports,
        Item::Regfile(r) => &r.ports,
        Item::Pipeline(p) => &p.ports,
        Item::Linklist(l) => &l.ports,
        Item::Synchronizer(s) => &s.ports,
        Item::Clkgate(c) => &c.ports,
        Item::Template(t) => &t.ports,
        _ => &[],
    }
}

fn line_col(src: &str, byte: usize) -> (usize, usize) {
    let clamped = byte.min(src.len());
    let mut line = 1_usize;
    let mut col = 1_usize;
    for (idx, ch) in src.char_indices() {
        if idx >= clamped {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

fn rel_path_from(root: &Path, path: &str) -> String {
    let p = Path::new(path);
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let display = p
        .canonicalize()
        .ok()
        .and_then(|abs| abs.strip_prefix(&root).ok().map(|r| r.to_path_buf()))
        .unwrap_or_else(|| p.to_path_buf());
    normalize_path(&display)
}

fn abs_path(path: &str) -> String {
    Path::new(path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(path))
        .to_string_lossy()
        .to_string()
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn replace_ext(path: &str, ext: &str) -> String {
    let mut p = PathBuf::from(path);
    p.set_extension(ext);
    normalize_path(&p)
}

fn sibling_file(path: &str, file: &str) -> String {
    let p = Path::new(path);
    p.parent()
        .map(|dir| normalize_path(&dir.join(file)))
        .unwrap_or_else(|| file.to_string())
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn file_id(rel: &str) -> String {
    format!("file:{rel}")
}

fn construct_id(rel: &str, kind: &str, name: &str) -> String {
    format!("construct:{rel}#{kind}:{name}")
}

fn port_id(rel: &str, scope: &str, name: &str) -> String {
    format!("symbol:{rel}#{scope}:port:{name}")
}

fn param_id(rel: &str, scope: &str, name: &str) -> String {
    format!("symbol:{rel}#{scope}:param:{name}")
}

fn signal_id(rel: &str, scope: &str, name: &str) -> String {
    format!("symbol:{rel}#{scope}:signal:{name}")
}

fn instance_id(rel: &str, scope: &str, name: &str) -> String {
    format!("symbol:{rel}#{scope}:instance:{name}")
}

fn function_id(rel: &str, scope: &str, name: &str) -> String {
    format!("symbol:{rel}#{scope}:function:{name}")
}

fn method_id(rel: &str, scope: &str, name: &str) -> String {
    format!("symbol:{rel}#{scope}:method:{name}")
}

fn symbol_key(rel: &str, scope: &str, name: &str) -> String {
    format!("{rel}::{scope}::{name}")
}

fn unresolved_id(name: &str) -> String {
    format!("symbol:<unresolved>:{name}")
}

fn join_docs(outer: Option<&str>, inner: Option<&str>) -> Option<String> {
    let parts: Vec<_> = [outer, inner]
        .into_iter()
        .flatten()
        .filter(|s| !s.trim().is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join("\n"))
}

fn file_inner_doc(src: &str) -> Option<String> {
    let mut lines = Vec::new();
    for line in src.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("//!") {
            lines.push(rest.strip_prefix(' ').unwrap_or(rest).to_string());
        } else if trimmed.is_empty() {
            continue;
        } else {
            break;
        }
    }
    (!lines.is_empty()).then(|| lines.join("\n"))
}

fn artifact_attrs(rel: &str) -> BTreeMap<String, String> {
    let mut attrs = BTreeMap::new();
    attrs.insert("exists".to_string(), Path::new(rel).exists().to_string());
    attrs
}

fn expr_signal_name(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Ident(n) | ExprKind::SynthIdent(n, _) => Some(n.clone()),
        ExprKind::FieldAccess(base, field) => {
            expr_signal_name(base).map(|b| format!("{b}.{}", field.name))
        }
        ExprKind::Index(base, _) => expr_signal_name(base),
        ExprKind::BitSlice(base, _, _)
        | ExprKind::PartSelect(base, _, _, _)
        | ExprKind::LatencyAt(base, _) => expr_signal_name(base),
        _ => None,
    }
}

fn expr_label(expr: &Expr) -> String {
    expr_signal_name(expr).unwrap_or_else(|| format!("expr@{}", expr.span.start))
}

fn receiver_label(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Ident(n) | ExprKind::SynthIdent(n, _) => Some(n.clone()),
        ExprKind::FieldAccess(base, field) => {
            receiver_label(base).map(|b| format!("{b}.{}", field.name))
        }
        ExprKind::Index(base, _) => receiver_label(base),
        ExprKind::BitSlice(base, _, _)
        | ExprKind::PartSelect(base, _, _, _)
        | ExprKind::LatencyAt(base, _) => receiver_label(base),
        _ => None,
    }
}

fn tokenize(s: &str) -> Vec<String> {
    s.to_ascii_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| !w.is_empty() && w.len() >= 2)
        .map(|w| w.to_string())
        .collect()
}

fn node_map(index: &GraphIndex) -> BTreeMap<String, &NodeRecord> {
    index.nodes.iter().map(|n| (n.id.clone(), n)).collect()
}

fn span_start(node: &NodeRecord) -> usize {
    node.span
        .as_ref()
        .map(|s| s.byte_start)
        .unwrap_or(usize::MAX)
}

fn span_info_start(span: &Option<SpanInfo>) -> usize {
    span.as_ref().map(|s| s.byte_start).unwrap_or(usize::MAX)
}

fn location_label(node: &NodeRecord) -> String {
    if let Some(span) = &node.span {
        format!("{}:{}", node.file, span.line_start)
    } else {
        node.file.clone()
    }
}

fn edge_location_label(edge: &EdgeRecord, fallback: &NodeRecord) -> String {
    if let Some(span) = &edge.span {
        format!("{}:{}", fallback.file, span.line_start)
    } else {
        location_label(fallback)
    }
}
