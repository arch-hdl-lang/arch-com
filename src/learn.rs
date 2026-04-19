//! Compiler learning system (v1).
//!
//! Captures error→fix pairs locally, builds a lexical index, and answers
//! `arch advise <query>` lookups. All data stays on-device under
//! `~/.arch/learn/`. See `doc/plan_arch_learning_system.md` for the roadmap.
//!
//! v1 is deliberately minimal:
//! - JSONL event stream (hand-written JSON serde, no serde_json dep)
//! - Pending-failure tracking per source file
//! - BM25 lexical index over error message + diff summary
//! - No embeddings, no network, no sharing mechanism
//!
//! Data layout:
//! ```text
//! ~/.arch/learn/
//!   ├── events.jsonl            append-only capture stream
//!   ├── index.json              BM25 index built by `arch learn-index`
//!   ├── pending/<hash>.json     in-flight failure per source file
//!   └── .first_run_notice       marker file for one-time privacy notice
//! ```

use std::fs;
use std::io::Write;
use std::path::PathBuf;

/// One recorded learning event. v1 only emits `kind: "error_fix"`.
#[derive(Debug, Clone)]
pub struct Event {
    pub ts: String,          // ISO-8601 UTC
    pub kind: String,        // "error_fix"
    pub error_code: String,  // e.g. "width_mismatch"
    pub error_message: String,
    pub file_path: String,
    pub src_before: String,
    pub src_after: String,
    pub diff_summary: String, // short one-line summary of what changed
}

/// Pending failure — written on `arch check --learn` failure, consumed on
/// the next successful `arch check --learn` on the same file.
#[derive(Debug, Clone)]
struct PendingFailure {
    ts: String,
    error_code: String,
    error_message: String,
    src: String,
}

/// Resolve the learning directory, creating it if missing.
pub fn learn_dir() -> std::io::Result<PathBuf> {
    let home = std::env::var("HOME")
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::NotFound, "$HOME not set"))?;
    let dir = PathBuf::from(home).join(".arch").join("learn");
    fs::create_dir_all(&dir)?;
    fs::create_dir_all(dir.join("pending"))?;
    Ok(dir)
}

/// Is learning capture enabled? Honors `ARCH_NO_LEARN=1` as an opt-out.
pub fn is_enabled() -> bool {
    match std::env::var("ARCH_NO_LEARN") {
        Ok(v) => !(v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes")),
        Err(_) => true,
    }
}

/// Maximum store size in bytes. Default 100 MB; override with
/// `ARCH_LEARN_MAX_MB=<n>` (non-negative integer).
pub fn max_bytes() -> u64 {
    let mb = std::env::var("ARCH_LEARN_MAX_MB")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(100);
    mb.saturating_mul(1024 * 1024)
}

fn store_size_bytes(dir: &std::path::Path) -> u64 {
    fn walk(p: &std::path::Path) -> u64 {
        let mut total = 0u64;
        if let Ok(md) = fs::symlink_metadata(p) {
            if md.file_type().is_dir() {
                if let Ok(rd) = fs::read_dir(p) {
                    for entry in rd.flatten() {
                        total = total.saturating_add(walk(&entry.path()));
                    }
                }
            } else if md.file_type().is_file() {
                total = md.len();
            }
        }
        total
    }
    walk(dir)
}

/// Print a one-time privacy notice the first time learning capture runs.
pub fn maybe_print_first_run_notice() -> std::io::Result<()> {
    let dir = learn_dir()?;
    let marker = dir.join(".first_run_notice");
    if marker.exists() {
        return Ok(());
    }
    eprintln!();
    eprintln!("📚 ARCH learning capture is ON (always-on; errors recorded locally).");
    eprintln!("   Data stored locally at: {}", dir.display());
    eprintln!("   Nothing is transmitted off-device.");
    eprintln!("   Opt out: set ARCH_NO_LEARN=1.  Cap: ARCH_LEARN_MAX_MB (default 100).");
    eprintln!("   `arch advise <query>` retrieves similar past errors.");
    eprintln!();
    fs::write(&marker, "")?;
    Ok(())
}

/// Check the store size. Warns at ≥90% of cap; returns `false` if at/over
/// cap (caller should skip writes). Prints at most one warning per process.
pub fn check_capacity() -> bool {
    use std::sync::atomic::{AtomicBool, Ordering};
    static WARNED: AtomicBool = AtomicBool::new(false);
    let dir = match learn_dir() {
        Ok(d) => d,
        Err(_) => return false,
    };
    let size = store_size_bytes(&dir);
    let cap = max_bytes();
    if cap == 0 {
        return false;
    }
    if size >= cap {
        if !WARNED.swap(true, Ordering::Relaxed) {
            eprintln!(
                "⚠️  ARCH learn store is full ({} / {} MB). New events skipped. \
                 Raise cap via ARCH_LEARN_MAX_MB or run `arch learn-clear`.",
                size / (1024 * 1024),
                cap / (1024 * 1024),
            );
        }
        return false;
    }
    if size * 10 >= cap * 9 && !WARNED.swap(true, Ordering::Relaxed) {
        eprintln!(
            "⚠️  ARCH learn store is {}% full ({} / {} MB). \
             Raise cap via ARCH_LEARN_MAX_MB or run `arch learn-clear`.",
            (size * 100 / cap),
            size / (1024 * 1024),
            cap / (1024 * 1024),
        );
    }
    true
}

/// Delete the entire local learning store.
pub fn clear_store() -> std::io::Result<()> {
    let home = std::env::var("HOME")
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::NotFound, "$HOME not set"))?;
    let dir = PathBuf::from(home).join(".arch").join("learn");
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    Ok(())
}

/// Short hash of a file path to name the pending file.
fn path_hash(s: &str) -> String {
    // FNV-1a 64-bit. Plenty unique for hundreds of project files.
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

/// Record a compile failure. Called from `arch check --learn` when a check
/// fails. Writes a per-file pending-failure record that will be matched with
/// the next successful check on the same file.
pub fn record_failure(
    file_path: &str,
    error_code: &str,
    error_message: &str,
    src: &str,
) -> std::io::Result<()> {
    if !check_capacity() {
        return Ok(());
    }
    let dir = learn_dir()?;
    let pending_file = dir.join("pending").join(format!("{}.json", path_hash(file_path)));
    let ts = iso8601_now();
    let pending = PendingFailure {
        ts,
        error_code: error_code.to_string(),
        error_message: error_message.to_string(),
        src: src.to_string(),
    };
    fs::write(&pending_file, pending_to_json(&pending))?;
    Ok(())
}

/// If a pending failure exists for this file, emit an error_fix event
/// comparing its stored src with the current (successful) src, then delete
/// the pending entry. Returns the event if one was emitted.
pub fn record_success_if_pending(
    file_path: &str,
    src_after: &str,
) -> std::io::Result<Option<Event>> {
    let dir = learn_dir()?;
    let pending_file = dir.join("pending").join(format!("{}.json", path_hash(file_path)));
    if !pending_file.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&pending_file)?;
    let pending = match json_to_pending(&raw) {
        Some(p) => p,
        None => {
            // Corrupt pending; drop it and move on.
            let _ = fs::remove_file(&pending_file);
            return Ok(None);
        }
    };
    // No-op if the source didn't actually change (e.g. transient error env).
    if pending.src == src_after {
        let _ = fs::remove_file(&pending_file);
        return Ok(None);
    }
    let diff_summary = short_diff_summary(&pending.src, src_after);
    let event = Event {
        ts: iso8601_now(),
        kind: "error_fix".to_string(),
        error_code: pending.error_code,
        error_message: pending.error_message,
        file_path: file_path.to_string(),
        src_before: pending.src,
        src_after: src_after.to_string(),
        diff_summary,
    };
    append_event(&event)?;
    let _ = fs::remove_file(&pending_file);
    Ok(Some(event))
}

fn append_event(e: &Event) -> std::io::Result<()> {
    let dir = learn_dir()?;
    let path = dir.join("events.jsonl");
    let mut f = fs::OpenOptions::new().create(true).append(true).open(&path)?;
    writeln!(f, "{}", event_to_json(e))?;
    Ok(())
}

/// Build / rebuild the BM25 index over events.jsonl. Writes index.json.
pub fn build_index() -> std::io::Result<usize> {
    let dir = learn_dir()?;
    let events_path = dir.join("events.jsonl");
    if !events_path.exists() {
        eprintln!("No events to index ({} does not exist).", events_path.display());
        return Ok(0);
    }
    let raw = fs::read_to_string(&events_path)?;
    let events: Vec<Event> = raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(json_to_event)
        .collect();

    // Compute document frequencies.
    let n_docs = events.len();
    let mut df: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut doc_terms: Vec<Vec<String>> = Vec::with_capacity(n_docs);
    for e in &events {
        let text = format!(
            "{} {} {}",
            e.error_code, e.error_message, e.diff_summary
        );
        let terms = tokenize(&text);
        let uniq: std::collections::HashSet<_> = terms.iter().cloned().collect();
        for t in uniq {
            *df.entry(t).or_insert(0) += 1;
        }
        doc_terms.push(terms);
    }

    // Write index as a minimal JSON blob: n_docs, avg_dl, term→df map.
    let avg_dl: f64 = if n_docs == 0 {
        0.0
    } else {
        doc_terms.iter().map(|t| t.len()).sum::<usize>() as f64 / n_docs as f64
    };
    let index_path = dir.join("index.json");
    let mut out = String::from("{");
    out.push_str(&format!("\"n_docs\":{},", n_docs));
    out.push_str(&format!("\"avg_dl\":{},", avg_dl));
    out.push_str("\"df\":{");
    let mut first = true;
    for (term, count) in &df {
        if !first { out.push(','); }
        first = false;
        out.push_str(&format!("\"{}\":{}", escape_json_string(term), count));
    }
    out.push_str("}}");
    fs::write(&index_path, out)?;
    Ok(n_docs)
}

/// A query result from `arch advise`.
pub struct Match {
    pub score: f64,
    pub event: Event,
    pub retrieved_count: u32,
}

/// Stable, order-independent fingerprint for an event — used as the key in
/// `retrieval_counts.json`. Hash ts + error_code + diff_summary so the id
/// survives any future field additions without breaking existing counts.
pub fn event_id(e: &Event) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for chunk in [e.ts.as_bytes(), e.error_code.as_bytes(), e.diff_summary.as_bytes()] {
        for b in chunk {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h ^= b'|' as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

fn counts_path() -> std::io::Result<PathBuf> {
    Ok(learn_dir()?.join("retrieval_counts.json"))
}

fn load_counts() -> std::io::Result<std::collections::HashMap<String, u32>> {
    let path = counts_path()?;
    if !path.exists() {
        return Ok(std::collections::HashMap::new());
    }
    let raw = fs::read_to_string(&path)?;
    let mut out = std::collections::HashMap::new();
    // Tiny parser: `{"id":N,"id":N,...}`
    let trimmed = raw.trim().trim_start_matches('{').trim_end_matches('}');
    for entry in trimmed.split(',') {
        let entry = entry.trim();
        if entry.is_empty() { continue; }
        if let Some((k, v)) = entry.split_once(':') {
            let k = k.trim().trim_matches('"').to_string();
            if let Ok(n) = v.trim().parse::<u32>() {
                out.insert(k, n);
            }
        }
    }
    Ok(out)
}

fn save_counts(counts: &std::collections::HashMap<String, u32>) -> std::io::Result<()> {
    let path = counts_path()?;
    let mut s = String::from("{");
    let mut first = true;
    for (k, v) in counts {
        if !first { s.push(','); }
        first = false;
        s.push_str(&format!("\"{}\":{}", k, v));
    }
    s.push('}');
    fs::write(&path, s)
}

fn bump_counts(ids: &[String]) -> std::io::Result<()> {
    if ids.is_empty() { return Ok(()); }
    let mut counts = load_counts()?;
    for id in ids {
        *counts.entry(id.clone()).or_insert(0) += 1;
    }
    save_counts(&counts)
}

/// Quick advise that does not bump retrieval counts — used by the inline
/// compile-failure hint in `arch check` to avoid inflating counts on
/// suggestions the user never actually looked at.
pub fn peek(query: &str, k: usize) -> std::io::Result<Vec<Match>> {
    advise_impl(query, k, false)
}

/// Load events, tokenize the query, score each event via BM25, return top-K.
pub fn advise(query: &str, k: usize) -> std::io::Result<Vec<Match>> {
    advise_impl(query, k, true)
}

fn advise_impl(query: &str, k: usize, bump: bool) -> std::io::Result<Vec<Match>> {
    let dir = learn_dir()?;
    let events_path = dir.join("events.jsonl");
    if !events_path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(&events_path)?;
    let events: Vec<Event> = raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(json_to_event)
        .collect();

    let index_path = dir.join("index.json");
    let (n_docs, avg_dl, df) = if index_path.exists() {
        let index_raw = fs::read_to_string(&index_path)?;
        parse_index(&index_raw)
    } else {
        // Fall back to computing on the fly.
        (events.len(), 0.0, std::collections::HashMap::new())
    };

    let q_terms = tokenize(query);
    let k1 = 1.5_f64;
    let b = 0.75_f64;
    let mut scored: Vec<(f64, Event)> = Vec::with_capacity(events.len());
    for e in events {
        let text = format!(
            "{} {} {}",
            e.error_code, e.error_message, e.diff_summary
        );
        let d_terms = tokenize(&text);
        let dl = d_terms.len() as f64;
        let mut score = 0.0_f64;
        for qt in &q_terms {
            let tf = d_terms.iter().filter(|t| *t == qt).count() as f64;
            if tf == 0.0 {
                continue;
            }
            let df_t = *df.get(qt).unwrap_or(&1) as f64;
            let idf = (((n_docs as f64 - df_t + 0.5) / (df_t + 0.5)) + 1.0).ln();
            let denom = tf + k1 * (1.0 - b + b * (dl / avg_dl.max(1.0)));
            score += idf * (tf * (k1 + 1.0)) / denom;
        }
        if score > 0.0 {
            scored.push((score, e));
        }
    }
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(k);

    let counts = load_counts().unwrap_or_default();
    let top: Vec<Match> = scored.into_iter().map(|(s, e)| {
        let id = event_id(&e);
        let c = *counts.get(&id).unwrap_or(&0);
        Match { score: s, event: e, retrieved_count: c }
    }).collect();

    if bump {
        let ids: Vec<String> = top.iter().map(|m| event_id(&m.event)).collect();
        let _ = bump_counts(&ids);
    }

    Ok(top)
}

/// Print stored stats.
pub fn print_stats() -> std::io::Result<()> {
    let dir = learn_dir()?;
    let events_path = dir.join("events.jsonl");
    if !events_path.exists() {
        println!("No events captured yet. Run `arch check --learn <file.arch>` to start.");
        return Ok(());
    }
    let raw = fs::read_to_string(&events_path)?;
    let events: Vec<Event> = raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(json_to_event)
        .collect();
    println!("Learning store: {}", dir.display());
    println!("Events:         {}", events.len());
    let mut by_code: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for e in &events {
        *by_code.entry(e.error_code.clone()).or_insert(0) += 1;
    }
    if !by_code.is_empty() {
        println!();
        println!("By error code:");
        let mut pairs: Vec<_> = by_code.iter().collect();
        pairs.sort_by(|a, b| b.1.cmp(a.1));
        for (c, n) in pairs {
            println!("  {:4}  {}", n, c);
        }
    }
    Ok(())
}

/// Prune events from the store. Returns (kept, removed).
/// An event is removed if it matches *any* of the filters:
/// - `code == Some(c)`: event's error_code equals `c`
/// - `substr == Some(s)`: `s` appears in diff_summary, error_message, or file_path
/// - `older_than_days == Some(d)`: event timestamp is older than `d` days ago
/// If `dry_run` is true, nothing is written; just counts.
pub fn prune(
    code: Option<&str>,
    substr: Option<&str>,
    older_than_days: Option<u64>,
    dry_run: bool,
) -> std::io::Result<(usize, usize)> {
    let dir = learn_dir()?;
    let events_path = dir.join("events.jsonl");
    if !events_path.exists() {
        return Ok((0, 0));
    }
    let raw = fs::read_to_string(&events_path)?;
    let cutoff_ts: Option<String> = older_than_days.map(|d| {
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|x| x.as_secs())
            .unwrap_or(0);
        let cutoff = now_secs.saturating_sub(d * 86400);
        let (y, mo, da, hh, mm, ss) = epoch_to_utc(cutoff);
        format!("{y:04}-{mo:02}-{da:02}T{hh:02}:{mm:02}:{ss:02}Z")
    });

    let mut kept_lines: Vec<String> = Vec::new();
    let mut removed = 0usize;
    for line in raw.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let ev = match json_to_event(line) {
            Some(e) => e,
            None => {
                kept_lines.push(line.to_string());
                continue;
            }
        };
        let mut drop = false;
        if let Some(c) = code {
            if ev.error_code == c {
                drop = true;
            }
        }
        if !drop {
            if let Some(s) = substr {
                if ev.diff_summary.contains(s)
                    || ev.error_message.contains(s)
                    || ev.file_path.contains(s)
                {
                    drop = true;
                }
            }
        }
        if !drop {
            if let Some(cutoff) = &cutoff_ts {
                if ev.ts.as_str() < cutoff.as_str() {
                    drop = true;
                }
            }
        }
        if drop {
            removed += 1;
        } else {
            kept_lines.push(line.to_string());
        }
    }
    let kept = kept_lines.len();
    if !dry_run && removed > 0 {
        let mut out = kept_lines.join("\n");
        if !out.is_empty() {
            out.push('\n');
        }
        fs::write(&events_path, out)?;
        // Index is now stale; remove so `advise` rebuilds / warns.
        let _ = fs::remove_file(dir.join("index.json"));
    }
    Ok((kept, removed))
}

// ── helpers ──────────────────────────────────────────────────────────────

fn iso8601_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Formatting an ISO-8601 timestamp without chrono.
    let (y, m, d, hh, mm, ss) = epoch_to_utc(secs);
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

fn epoch_to_utc(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    // Naive UTC conversion; correct for our purposes (post-1970, no leap seconds).
    let ss = (secs % 60) as u32;
    let mm = ((secs / 60) % 60) as u32;
    let hh = ((secs / 3600) % 24) as u32;
    let days = (secs / 86400) as u32;
    let mut year: u32 = 1970;
    let mut rem = days;
    loop {
        let ly = is_leap(year);
        let year_days = if ly { 366 } else { 365 };
        if rem < year_days { break; }
        rem -= year_days;
        year += 1;
    }
    let ly = is_leap(year);
    let months = [31u32, if ly { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 0u32;
    for (i, &ml) in months.iter().enumerate() {
        if rem < ml {
            month = i as u32;
            break;
        }
        rem -= ml;
    }
    (year, month + 1, rem + 1, hh, mm, ss)
}

fn is_leap(y: u32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

fn tokenize(s: &str) -> Vec<String> {
    s.to_ascii_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| !w.is_empty() && w.len() >= 2)
        .map(|w| w.to_string())
        .collect()
}

fn short_diff_summary(before: &str, after: &str) -> String {
    // Find first differing line; return "<before>  →  <after>".
    let b: Vec<&str> = before.lines().collect();
    let a: Vec<&str> = after.lines().collect();
    for (bl, al) in b.iter().zip(a.iter()) {
        if bl != al {
            return format!("{}  →  {}", bl.trim(), al.trim());
        }
    }
    if a.len() > b.len() {
        format!("(added) {}", a[b.len()].trim())
    } else if b.len() > a.len() {
        format!("(removed) {}", b[a.len()].trim())
    } else {
        "(no line-level diff)".to_string()
    }
}

// Minimal JSON serialization — hand-written because we don't depend on
// serde_json. All fields are strings; values are escaped. No nested objects.

fn escape_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn event_to_json(e: &Event) -> String {
    format!(
        "{{\"ts\":\"{}\",\"kind\":\"{}\",\"error_code\":\"{}\",\"error_message\":\"{}\",\"file_path\":\"{}\",\"src_before\":\"{}\",\"src_after\":\"{}\",\"diff_summary\":\"{}\"}}",
        escape_json_string(&e.ts),
        escape_json_string(&e.kind),
        escape_json_string(&e.error_code),
        escape_json_string(&e.error_message),
        escape_json_string(&e.file_path),
        escape_json_string(&e.src_before),
        escape_json_string(&e.src_after),
        escape_json_string(&e.diff_summary),
    )
}

fn pending_to_json(p: &PendingFailure) -> String {
    format!(
        "{{\"ts\":\"{}\",\"error_code\":\"{}\",\"error_message\":\"{}\",\"src\":\"{}\"}}",
        escape_json_string(&p.ts),
        escape_json_string(&p.error_code),
        escape_json_string(&p.error_message),
        escape_json_string(&p.src),
    )
}

// Minimal JSON parser — handles only the shapes we emit. Flat objects with
// string values. Returns None on any unexpected structure.

fn parse_json_string(input: &[u8], pos: &mut usize) -> Option<String> {
    skip_ws(input, pos);
    if *pos >= input.len() || input[*pos] != b'"' { return None; }
    *pos += 1;
    let mut out = String::new();
    while *pos < input.len() {
        let c = input[*pos];
        if c == b'"' { *pos += 1; return Some(out); }
        if c == b'\\' {
            *pos += 1;
            if *pos >= input.len() { return None; }
            match input[*pos] {
                b'"' => out.push('"'),
                b'\\' => out.push('\\'),
                b'/' => out.push('/'),
                b'n' => out.push('\n'),
                b'r' => out.push('\r'),
                b't' => out.push('\t'),
                b'u' => {
                    if *pos + 4 >= input.len() { return None; }
                    let hex = std::str::from_utf8(&input[*pos+1..*pos+5]).ok()?;
                    let code = u32::from_str_radix(hex, 16).ok()?;
                    out.push(char::from_u32(code)?);
                    *pos += 4;
                }
                _ => return None,
            }
            *pos += 1;
        } else {
            // Multi-byte UTF-8: push raw bytes
            let end = next_utf8(input, *pos);
            let slice = &input[*pos..end];
            out.push_str(std::str::from_utf8(slice).ok()?);
            *pos = end;
        }
    }
    None
}

fn next_utf8(b: &[u8], pos: usize) -> usize {
    let c = b[pos];
    let len = if c < 0x80 { 1 }
        else if c < 0xc0 { 1 }
        else if c < 0xe0 { 2 }
        else if c < 0xf0 { 3 }
        else { 4 };
    (pos + len).min(b.len())
}

fn skip_ws(input: &[u8], pos: &mut usize) {
    while *pos < input.len() && matches!(input[*pos], b' ' | b'\t' | b'\n' | b'\r') {
        *pos += 1;
    }
}

fn expect_char(input: &[u8], pos: &mut usize, c: u8) -> Option<()> {
    skip_ws(input, pos);
    if *pos >= input.len() || input[*pos] != c { return None; }
    *pos += 1;
    Some(())
}

fn parse_object_strings(input: &[u8], pos: &mut usize) -> Option<std::collections::HashMap<String, String>> {
    expect_char(input, pos, b'{')?;
    let mut map = std::collections::HashMap::new();
    skip_ws(input, pos);
    if *pos < input.len() && input[*pos] == b'}' { *pos += 1; return Some(map); }
    loop {
        let key = parse_json_string(input, pos)?;
        expect_char(input, pos, b':')?;
        let value = parse_json_string(input, pos)?;
        map.insert(key, value);
        skip_ws(input, pos);
        if *pos >= input.len() { return None; }
        match input[*pos] {
            b',' => { *pos += 1; }
            b'}' => { *pos += 1; return Some(map); }
            _ => return None,
        }
    }
}

fn json_to_event(line: &str) -> Option<Event> {
    let b = line.as_bytes();
    let mut pos = 0;
    let map = parse_object_strings(b, &mut pos)?;
    Some(Event {
        ts: map.get("ts")?.clone(),
        kind: map.get("kind")?.clone(),
        error_code: map.get("error_code")?.clone(),
        error_message: map.get("error_message")?.clone(),
        file_path: map.get("file_path").cloned().unwrap_or_default(),
        src_before: map.get("src_before")?.clone(),
        src_after: map.get("src_after")?.clone(),
        diff_summary: map.get("diff_summary").cloned().unwrap_or_default(),
    })
}

fn json_to_pending(raw: &str) -> Option<PendingFailure> {
    let b = raw.as_bytes();
    let mut pos = 0;
    let map = parse_object_strings(b, &mut pos)?;
    Some(PendingFailure {
        ts: map.get("ts")?.clone(),
        error_code: map.get("error_code")?.clone(),
        error_message: map.get("error_message")?.clone(),
        src: map.get("src")?.clone(),
    })
}

/// Parse the index.json written by `build_index`. Returns (n_docs, avg_dl, df_map).
/// Tolerant: on any parse error, returns (0, 0.0, empty).
fn parse_index(raw: &str) -> (usize, f64, std::collections::HashMap<String, usize>) {
    // Quick-and-dirty: find "n_docs": <int>, "avg_dl": <float>, "df": { strings→ints }.
    let n_docs = scrape_usize(raw, "\"n_docs\":").unwrap_or(0);
    let avg_dl = scrape_f64(raw, "\"avg_dl\":").unwrap_or(0.0);
    let mut df = std::collections::HashMap::new();
    if let Some(pos) = raw.find("\"df\":{") {
        let after = &raw[pos + "\"df\":{".len()..];
        let b = after.as_bytes();
        let mut p = 0usize;
        loop {
            skip_ws(b, &mut p);
            if p >= b.len() || b[p] == b'}' { break; }
            let key = match parse_json_string(b, &mut p) { Some(k) => k, None => break };
            if expect_char(b, &mut p, b':').is_none() { break; }
            skip_ws(b, &mut p);
            let start = p;
            while p < b.len() && (b[p].is_ascii_digit()) { p += 1; }
            let n: usize = match std::str::from_utf8(&b[start..p]).ok().and_then(|s| s.parse().ok()) {
                Some(n) => n, None => break
            };
            df.insert(key, n);
            skip_ws(b, &mut p);
            if p < b.len() && b[p] == b',' { p += 1; }
        }
    }
    (n_docs, avg_dl, df)
}

fn scrape_usize(raw: &str, key: &str) -> Option<usize> {
    let pos = raw.find(key)? + key.len();
    let rest = &raw[pos..];
    let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    rest[..end].parse().ok()
}

fn scrape_f64(raw: &str, key: &str) -> Option<f64> {
    let pos = raw.find(key)? + key.len();
    let rest = &raw[pos..];
    let end = rest.find(|c: char| !c.is_ascii_digit() && c != '.' && c != '-').unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// Classify a compiler error message into a short error code for indexing.
/// Heuristic: look at the first few words of the message.
pub fn classify_error(msg: &str) -> String {
    let lower = msg.to_ascii_lowercase();
    if lower.contains("width mismatch") || lower.contains("arithmetic widening") {
        "width_mismatch".to_string()
    } else if lower.contains("undefined") && (lower.contains("signal") || lower.contains("module") || lower.contains("port") || lower.contains("name")) {
        "undefined_name".to_string()
    } else if lower.contains("ambiguous precedence") {
        "precedence".to_string()
    } else if lower.contains("multiple drivers") {
        "multi_driver".to_string()
    } else if lower.contains("unexpected token") || lower.contains("expected") {
        "parse_error".to_string()
    } else if lower.contains("duplicate") {
        "duplicate".to_string()
    } else if lower.contains("type mismatch") {
        "type_mismatch".to_string()
    } else if lower.contains("divide by zero") || lower.contains("division by zero") {
        "div_zero".to_string()
    } else if lower.contains("guard signal") {
        "guard".to_string()
    } else if lower.contains("clock domain") || lower.contains("cdc") {
        "cdc".to_string()
    } else {
        "other".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_basic() {
        let tokens = tokenize("width mismatch: UInt<8> vs UInt<9>");
        assert!(tokens.contains(&"width".to_string()));
        assert!(tokens.contains(&"mismatch".to_string()));
        assert!(tokens.contains(&"uint".to_string()));
    }

    #[test]
    fn event_roundtrip() {
        let e = Event {
            ts: "2026-04-18T00:00:00Z".to_string(),
            kind: "error_fix".to_string(),
            error_code: "width_mismatch".to_string(),
            error_message: "RHS is UInt<9> but LHS is UInt<8>".to_string(),
            file_path: "/tmp/foo.arch".to_string(),
            src_before: "cnt <= cnt + 1;".to_string(),
            src_after: "cnt <= (cnt + 1).trunc<8>();".to_string(),
            diff_summary: "cnt <= cnt + 1;  →  cnt <= (cnt + 1).trunc<8>();".to_string(),
        };
        let json = event_to_json(&e);
        let parsed = json_to_event(&json).expect("round trip");
        assert_eq!(parsed.error_code, e.error_code);
        assert_eq!(parsed.src_before, e.src_before);
        assert_eq!(parsed.src_after, e.src_after);
    }

    #[test]
    fn classify_examples() {
        assert_eq!(classify_error("width mismatch: UInt<8> vs UInt<9>"), "width_mismatch");
        assert_eq!(classify_error("undefined signal `foo`"), "undefined_name");
        assert_eq!(classify_error("ambiguous precedence: ..."), "precedence");
        assert_eq!(classify_error("something else"), "other");
    }
}
