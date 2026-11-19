"""Bounded, graph-independent search for compiler construct ownership.

This is a lexical discovery helper, not a Rust parser or semantic proof. It
never invokes a model, reads a feature map, or changes the graph-first default.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re

if __package__:
    from .compiler_context import ROLES, _read, _run, _source
    from .compiler_response import DECL
else:
    from compiler_context import ROLES, _read, _run, _source
    from compiler_response import DECL

POLICY = "compiler-search-v1"
STOPWORDS = set("""a an and are as at by each for from how in into is it of on or
the their these this its to using which with where what arch rust compiler construct
constructs feature features ast syntax tree parser parse parsing backend codegen
systemverilog emitter emit semantic checker check typechecker ownership owner
owners declaration declarations implementation implementations function functions
source sources citation citations symbol symbols path line role roles identify
locate find show return give one per corresponding please entry point""".split())


def feature_names(query: str) -> list[str]:
    """Prefer a named construct phrase; otherwise discard instruction vocabulary.

    Multiword names yield joined/snake-case spellings and their initialism.
    No language-feature aliases or graph metadata are consulted.
    """
    if not isinstance(query, str) or not query.strip() or len(query.encode()) > 8192:
        raise ValueError("query must contain 1..8192 UTF-8 bytes")
    phrases = list(re.finditer(r"\bARCH['\u2019]s\s+([^,.;:]+?)\s+construct\b", query, re.I))
    if len(phrases) > 1 or (phrases and re.search(r"\b(and|or)\b", phrases[0].group(1), re.I)):
        return []
    # Output instructions after the first sentence must not supply feature names.
    sentence = re.split(r"[.;!?]", query, maxsplit=1)[0]
    if phrases:
        phrase = phrases[0]
        if phrase.end() > len(sentence):
            return []
        outside = sentence[:phrase.start()] + sentence[phrase.end():]
        if any(word.lower() not in STOPWORDS for word in re.findall(r"[A-Za-z][A-Za-z_0-9]*", outside)):
            return []
    subject = phrases[0].group(1) if phrases else sentence
    words = [word.lower() for word in re.findall(r"[A-Za-z][A-Za-z_0-9]*", subject)
             if word.lower() not in STOPWORDS]
    words = list(dict.fromkeys(words))
    if not words or len(words) > 8:
        return []
    names = [words[0], words[0].replace("_", "")] if len(words) == 1 else []
    if 1 < len(words) <= 5:
        names += ["_".join(words), "".join(words), "".join(w[0] for w in words)]
    return list(dict.fromkeys(names))


def retrieval_budget(context: dict, *, total_operations: int = 6,
                     reserve_operations: int = 2) -> dict:
    """Count host retrieval operations, not validation reads or internal rg I/O."""
    if (type(total_operations) is not int or type(reserve_operations) is not int
            or not 0 <= reserve_operations <= total_operations <= 32):
        raise ValueError("invalid retrieval operation budget")
    reads = context.get("source_read_calls", 0)
    commands = context.get("commands", [])
    if type(reads) is not int or reads < 0 or not isinstance(commands, list):
        raise ValueError("invalid retrieval counters")
    used = reads + len(commands)
    return {"total_operations": total_operations, "reserve_operations": reserve_operations,
            "host_operations": used, "remaining_operations": max(0, total_operations - used),
            "reserve_preserved": used <= total_operations - reserve_operations}


def _role(path: str, kind: str, symbol: str) -> str | None:
    if (path == "src/ast.rs" or path.startswith("src/ast/")) and kind in ("struct", "enum"):
        return "ast"
    if kind == "fn" and (path == "src/parser.rs" or path.startswith("src/parser/")) and symbol.startswith("parse_"):
        return "parser"
    if kind == "fn" and path.startswith("src/codegen/") and symbol.startswith("emit_"):
        return "backend"
    if kind == "fn" and (path == "src/typecheck.rs" or path.startswith("src/typecheck/")) and symbol.startswith("check_"):
        return "semantic_check"
    return None


def retrieve_search_context(root: Path, query: str, *, roles=("ast", "parser", "backend"),
                            byte_budget: int = 4096, total_operations: int = 6,
                            reserve_operations: int = 2) -> dict:
    """One bounded declaration search, then source reads within the host allowance.

    Missing/ambiguous coverage returns no consumable payload. Failed calls still
    count. Callers own any recovery and must honor the returned remaining budget.
    Source hashes and literal rows support compiler_response, but no semantic role
    bindings are attached: naming/path conventions are only search heuristics.
    """
    root = Path(root).resolve(strict=True)
    names = feature_names(query)
    roles = tuple(roles)
    if not roles or len(set(roles)) != len(roles) or set(roles) - set(ROLES):
        raise ValueError("roles must be distinct supported compiler roles")
    if type(byte_budget) is not int or not 1 <= byte_budget <= 4096:
        raise ValueError("invalid payload byte budget")
    result = {"policy": POLICY, "route": "search", "status": "needs_search", "reason": None,
              "evidence_kind": "lexical_candidates",
              "payload": "", "payload_bytes": 0, "evidence": [], "source_hashes": {},
              "commands": [], "source_read_calls": 0, "source_read_bytes": 0,
              "index_read_calls": 0, "index_read_bytes": 0, "host_graph_calls": 0,
              "missing_roles": list(roles), "truncated": False, "feature_names": names}
    budget = retrieval_budget(result, total_operations=total_operations, reserve_operations=reserve_operations)
    allowance = total_operations - reserve_operations
    try:
        if not names:
            raise ValueError("no bounded construct name; refine the question")
        if allowance < 2:
            raise ValueError("insufficient host allowance for search and source verification")
        if (root / "src").is_symlink() or not (root / "src").is_dir():
            raise ValueError("source root must be a real directory")
        alternatives = "|".join(re.escape(name) for name in names)
        pattern = (r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:(?:async|unsafe)\s+)?"
                   r"(?:fn|struct|enum)\s+(?:parse_|emit_|check_)?(?:" + alternatives + r")(?:Decl)?\b")
        command = ["rg", "--no-config", "--color", "never", "--no-heading", "-n", "-i",
                   "--sort", "path", "--glob", "*.rs", "--", pattern, "src"]
        call = _run(command, root)
        result["commands"].append(call)
        if call["exit_code"] not in (0, 1) or call["error"] or call["output_clipped"]:
            result["truncated"] = bool(call["output_clipped"])
            raise ValueError("search failed or exceeded output bound")
        candidates = {}
        for text in call["stdout"].splitlines():
            match = re.fullmatch(r"(src/[^\s:]+\.rs):([1-9][0-9]*):(.*)", text)
            if match is None:
                raise ValueError("unsupported search output")
            relative, number, quote = match.groups()
            declaration = DECL.match(quote)
            if declaration is None:
                raise ValueError("search row is not a declaration")
            symbol = declaration.group(1)
            kind = re.search(r"\b(fn|struct|enum)\s+" + re.escape(symbol) + r"\b", quote)
            role = _role(relative, kind.group(1) if kind else "", symbol)
            if role in roles:
                stem = re.sub(r"^(parse_|emit_|check_)|Decl$", "", symbol)
                stem = stem.replace("_", "").lower()
                if stem not in {name.replace("_", "").lower() for name in names}:
                    raise ValueError("search returned an unrelated symbol")
                group = candidates.setdefault(stem, {role: {} for role in roles})
                group[role][(relative, int(number))] = {
                    "path": relative, "line": int(number), "symbol": symbol, "quote": quote}
        complete = [group for group in candidates.values() if all(group[role] for role in roles)]
        result["missing_roles"] = [role for role in roles if not any(g[role] for g in candidates.values())]
        if not complete:
            raise ValueError("search lacks a consistent construct for all requested roles")
        if len(complete) != 1 or any(len(rows) != 1 for rows in complete[0].values()):
            raise ValueError("ambiguous declaration candidates; refine the question")
        rows = [next(iter(complete[0][role].values())) for role in roles]
        paths = list(dict.fromkeys(row["path"] for row in rows))
        resolved = {relative: _source(root, relative) for relative in paths}
        if len(set(resolved.values())) > min(4, allowance - len(result["commands"])):
            raise ValueError("source verification would spend reserved recovery operations")
        sources, pins, cache = {}, {}, {}
        for relative in paths:
            if resolved[relative] not in cache:
                cache[resolved[relative]] = _read(resolved[relative], result)
            data = cache[resolved[relative]]
            sources[relative] = data.decode("utf-8").splitlines()
            pins[relative] = hashlib.sha256(data).hexdigest()
        payload = ""
        for row in rows:
            lines = sources[row["path"]]
            if row["line"] > len(lines) or lines[row["line"] - 1] != row["quote"]:
                raise ValueError("source changed since search")
            payload += f"{row['path']}:{row['line']}:{row['quote']}\n"
        if len(payload.encode()) > byte_budget:
            result["truncated"] = True
            raise ValueError("source payload exceeds byte budget")
        result.update(status="ready", payload=payload, payload_bytes=len(payload.encode()),
                      evidence=rows, source_hashes=pins)
    except (OSError, RuntimeError, ValueError, TypeError) as error:
        result["reason"] = str(error)
    result["budget"] = retrieval_budget(result, total_operations=budget["total_operations"],
                                        reserve_operations=budget["reserve_operations"])
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("query")
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--total-operations", type=int, default=6)
    parser.add_argument("--reserve-operations", type=int, default=2)
    args = parser.parse_args()
    print(json.dumps(retrieve_search_context(args.root, args.query, total_operations=args.total_operations,
                                            reserve_operations=args.reserve_operations), indent=2))


if __name__ == "__main__":
    main()
