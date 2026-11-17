#!/usr/bin/env python3
"""Offline host-preflight regression audit; never launches a model."""

import json
from pathlib import Path
import subprocess
import tempfile

from compiler_context import prepare_compiler_prompt, retrieve_compiler_context


QUESTIONS = (
    ("counter", "For ARCH counter declarations, locate the AST declaration type, the parser function that parses a counter declaration, and the SystemVerilog emitter for a counter. Cite them under ast, parser, and backend respectively.",
     ("CounterDecl", "parse_counter", "emit_counter")),
    ("regfile", "For ARCH register-file declarations, locate the AST declaration type, the parser function that parses a register-file declaration, and the SystemVerilog emitter for a register file. Cite them under ast, parser, and backend respectively.",
     ("RegfileDecl", "parse_regfile", "emit_regfile")),
)


def audit(root: Path, binary: Path) -> list[dict]:
    records = []
    with tempfile.TemporaryDirectory(prefix="arch-compiler-context-") as directory:
        index = Path(directory) / "index"
        subprocess.run([str(binary), "graph", "dev-index", "--root", str(root), "--out", str(index)],
                       cwd=root, check=True, capture_output=True, timeout=60)
        for name, query, symbols in QUESTIONS:
            prompt, result = prepare_compiler_prompt(root, binary, query, index=index)
            passed = (result["status"] == "ready" and result["host_graph_calls"] == 1
                      and result["consumption_check"]["fresh"] and len(result["commands"]) == 1
                      and result["commands"][0]["command"][3] == query
                      and {r["symbol"] for r in result["evidence"]} == set(symbols)
                      and all(symbol in prompt for symbol in symbols))
            records.append({"id": name, "pass": passed, "context": result})
        known = retrieve_compiler_context(root, Path("no-binary"), "Read the first source line.",
                                         source_path="src/ast.rs", line_limit=1, index=Path("missing"))
        records.append({"id": "known_file_bypass", "pass": known["status"] == "ready"
                        and known["commands"] == [] and known["index_read_calls"] == 0})
        miss = retrieve_compiler_context(root, binary, "nonexistent_zqx_feature", index=index)
        records.append({"id": "coverage_miss", "pass": miss["status"] == "needs_search" and not miss["payload"]})
        small = retrieve_compiler_context(root, binary, "counter", index=index, byte_budget=1)
        records.append({"id": "payload_bound", "pass": small["status"] == "needs_search" and not small["payload"]})
        nodes_path = index / "nodes.jsonl"
        original = nodes_path.read_text()
        nodes = [json.loads(line) for line in original.splitlines()]
        counter = next(node for node in nodes if node["name"] == "CounterDecl")
        counter["path"] = "src/parser.rs"
        counter["line"] = next(row["line"] for row in records[0]["context"]["evidence"] if row["role"] == "parser")
        nodes_path.write_text("".join(json.dumps(node) + "\n" for node in nodes))
        forged = retrieve_compiler_context(root, binary, "counter", index=index)
        records.append({"id": "reference_is_not_declaration", "pass": forged["status"] == "needs_search"
                        and forged["commands"][0]["exit_code"] != 0 and not forged["payload"]})
        nodes_path.write_text(original)
        manifest = index / "manifest.json"
        changed = json.loads(manifest.read_text())
        changed["sources"]["src/ast.rs"] = "0" * 64
        manifest.write_text(json.dumps(changed))
        stale = retrieve_compiler_context(root, binary, "counter", index=index)
        records.append({"id": "stale_index", "pass": stale["status"] == "needs_search"
                        and stale["host_graph_calls"] == 1 and not stale["payload"]})
    return records
