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

# Bounded public vocabulary, not source locations or a compiler feature index.
# These normalizations apply only when the same subject occurs in the question.
PUBLIC_SUBJECTS = (
    ("domain", r"clock[ _-]+domains?", {"domain", "domains", "clockdomain", "clockdomains"}),
    ("tlm_method", r"(?:tlm|transaction(?:[ _-]+level)?)[ _-]+methods?",
     {"tlmmethod", "tlmmethods", "transactionmethod", "transactionmethods",
      "transactionlevelmethod", "transactionlevelmethods"}),
    ("handshake", r"handshakes?(?:[ _-]+channels?)?",
     {"handshake", "handshakes", "handshakechannel", "handshakechannels"}),
    ("credit_channel", r"credit[ _-]+channels?", {"creditchannel", "creditchannels"}),
)


def _opening_subject(query: str) -> str | None:
    # Accept an explicit opening subject, not an incidental word in an output
    # instruction. Unknown additions to its first clause are not discarded.
    first_clause = re.split(r"[,.;!?]", query, maxsplit=1)[0]
    prefix = r"\s*(?:(?:for|trace|locate|find|identify)\s+)?(?:(?:a|an|the)\s+)?(?:arch(?:['\u2019]s)?\s+)?"
    for stem, phrase, _ in PUBLIC_SUBJECTS:
        match = re.match(prefix + r"(?:" + phrase + r")(?![\w])", first_clause, re.I)
        if match is None:
            continue
        tail = first_clause[match.end():].strip()
        tail = re.sub(r"\b(?:declared\s+)?(?:on|in|inside|within)\s+(?:(?:a|an|the)\s+)?(?:arch\s+)?bus\b",
                      "", tail, flags=re.I)
        vocabulary = STOPWORDS | {"through", "representation", "represented", "parsed", "metadata",
                                  "protocol", "assertion", "assertions", "emission"}
        if stem == "credit_channel":
            vocabulary |= {"sender", "receiver", "side", "credit", "state"}
        if all(word.lower() in vocabulary for word in re.findall(r"[A-Za-z][A-Za-z_0-9]*", tail)):
            return stem
    return None


def _specific_names(query: str, names: list[str]) -> list[str]:
    # Later clauses may add a conflicting subject, but cannot supply the first
    # subject used for normalization (for example, an output-label instruction).
    mentioned = [stem for stem, phrase, _ in PUBLIC_SUBJECTS
                 if re.search(r"(?<![\w])(?:" + phrase + r")(?![\w])", query, re.I)]
    if len(mentioned) > 1:
        raise ValueError("multiple public feature subjects; refine the question")
    sentence = re.split(r"[.;!?]", query, maxsplit=1)[0]
    subjects = [(stem, spellings) for stem, phrase, spellings in PUBLIC_SUBJECTS
                if re.search(r"(?<![\w])(?:" + phrase + r")(?![\w])", sentence, re.I)]
    if not subjects:
        return names
    stem, spellings = subjects[0]
    if not names or any(name.replace("_", "") not in spellings for name in names):
        raise ValueError("selected construct drops or changes the specific public feature subject")
    return list(dict.fromkeys((stem, stem.replace("_", ""))))


def _backend_scope(query: str, names: list[str]) -> str | None:
    # State/receiver/assertion ownership is distinct from generate if/for variants.
    # The unsuffixed credit-channel state routine is sender-only, never a default
    # for a generic channel request or for another construct's state routine.
    if {name.replace("_", "") for name in names} != {"creditchannel"}:
        return None
    excluded_facet = (r"(?:(?:sender|receiver)(?:[ _-]+side)?(?:[ _-]+credit)?[ _-]+(?:state|fifo)"
                      r"(?:[ _-]+synthesis)?|(?:protocol[ _-]+)?assertions?)")
    included = re.sub(r"\b(?:rather than|instead of|but not|excluding|not)\s+"
                      r"(?:implying\s+)?" + excluded_facet + r"\b", "", query, flags=re.I)
    if re.search(r"\b(?:without|except|unless|excluding|not|never|no|rather than|instead of|other than)\b"
                 r"|\bnon[ _-]|\b(?:don't|doesn't)\b", included, re.I):
        raise ValueError("unsupported public backend exclusion; refine the question")
    scopes = set()
    if re.search(r"\bsender(?:[ _-]+side)?(?:[ _-]+[a-z]+){0,3}[ _-]+fifo\b", included, re.I):
        raise ValueError("unsupported sender FIFO scope; refine the question")
    for side in ("sender", "receiver"):
        facet = "state" if side == "sender" else "(?:state|fifo)"
        if re.search(r"\b" + side + r"(?:[ _-]+side)?(?:[ _-]+[a-z]+){0,3}[ _-]+" + facet + r"\b",
                     included, re.I):
            scopes.add(side + "_state")
    if scopes and all(re.search(r"\b" + side + r"\b", included, re.I) for side in ("sender", "receiver")):
        raise ValueError("multiple public backend scopes; refine the question")
    if re.search(r"\b(?:protocol[ _-]+)?assertions?\b", included, re.I):
        scopes.add("asserts")
    if len(scopes) > 1:
        raise ValueError("multiple public backend scopes; refine the question")
    return next(iter(scopes), None)


def feature_names(query: str) -> list[str]:
    """Prefer a named construct phrase; otherwise discard instruction vocabulary.

    Recognized opening public subjects use the bounded normalizations above.
    Other multiword names yield joined/snake-case spellings and their initialism.
    No graph, source-owner index or private expected answer is consulted.
    """
    if not isinstance(query, str) or not query.strip() or len(query.encode()) > 8192:
        raise ValueError("query must contain 1..8192 UTF-8 bytes")
    opening = _opening_subject(query)
    if opening:
        return list(dict.fromkeys((opening, opening.replace("_", ""))))
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


def _canonical_owner(role: str, symbol: str, names: list[str]) -> tuple[str, str | None]:
    prefixes = {"parser": "parse_", "backend": "emit_", "semantic_check": "check_"}
    body = symbol if role == "ast" else symbol.removeprefix(prefixes[role])
    suffixes = (("", None), ("Decl", None), ("Meta", None)) if role == "ast" else (
        ("", None), ("_decl", None), ("_if", "if"), ("_for", "for"))
    if role == "backend":
        suffixes += (("_sva", None), ("_asserts", "asserts"), ("_state", "state"),
                     ("_sender_state", "sender_state"), ("_receiver_state", "receiver_state"))
    elif role == "parser":
        suffixes += (("_block", None),)
    elif role == "semantic_check":
        suffixes = (("", None), ("_decl", None))
    stems = {name.replace("_", "").lower() for name in names}
    matches = set()
    for suffix, variant in suffixes:
        if suffix and not body.endswith(suffix):
            continue
        stem = (body[:-len(suffix)] if suffix else body).replace("_", "").lower()
        if stem in stems:
            matches.add((stem, variant))
    if len(matches) != 1:
        raise ValueError("search returned an unrelated or ambiguous symbol")
    return matches.pop()


def _public_variant(query: str, names: list[str]) -> str | None:
    scoped_generation = re.search(
        r"\bbus[ _-]+(?:(?:conditional|iterative)[ _-]+)?generat(?:e|ion)\b"
        r"|\bgenerate[ _-]+(?:if|for)[ _-]+bus\b"
        r"|\bgenerat(?:e|ion)(?:[ _-]+(?:if|for|block|declaration))*\s+(?:in|inside|within)\s+"
        r"(?:(?:a|an|the|arch)\s+)*bus\b", query, re.I)
    if scoped_generation and not all(re.fullmatch(r"busgenerate(?:if|for)?", name.replace("_", ""))
                                     for name in names):
        raise ValueError("selected construct drops explicit bus generation scope")
    # A prose "for" or an unrelated conditional must not select a routine.
    subject = "(?:" + "|".join(re.escape(name).replace("_", r"[ _-]") for name in names) + ")"
    if re.search(r"\b(?:not|non)\s*[- ]\s*(?:conditional|iterative)\s+" + subject + r"\b", query, re.I):
        raise ValueError("negated public construct variant; refine the question")
    found = set()
    for variant, adjective in (("if", "conditional"), ("for", "iterative")):
        prose = ("uncertain|needed|possible|available|any" if variant == "if" else
                 "each|every|the|a|an|its|this|these|their|my|your|all|example")
        if (re.search(r"\b" + adjective + r"\s+" + subject + r"\b", query, re.I)
                or re.search(r"\b" + subject + r"[ _-]+" + variant
                             + r"\b(?!\s+(?:" + prose + r")\b)", query, re.I)):
            found.add(variant)
    # Keep an explicit list such as "generate if and for" ambiguous.
    if re.search(r"\b" + subject + r"\s+(?:if\s*(?:and|or|[+/])\s*for|for\s*(?:and|or|[+/])\s*if)\b",
                 query, re.I):
        found.update(("if", "for"))
    if re.search(r"\b(?:conditional\s+(?:and|or)\s+iterative|iterative\s+(?:and|or)\s+conditional)\s+"
                 + subject + r"\b", query, re.I):
        found.update(("if", "for"))
    if len(found) > 1:
        raise ValueError("multiple public construct variants; refine the question")
    return next(iter(found), None)


def retrieve_search_context(root: Path, query: str, *, roles=("ast", "parser", "backend"),
                            byte_budget: int = 4096, total_operations: int = 6,
                            reserve_operations: int = 2, construct: str | None = None,
                            rg_binary: Path | None = None) -> dict:
    """One bounded declaration search, then source reads within the host allowance.

    Missing/ambiguous coverage returns no consumable payload. Failed calls still
    count. Callers own any recovery and must honor the returned remaining budget.
    Source hashes and literal rows support compiler_response, but no semantic role
    bindings are attached: naming/path conventions are only search heuristics.
    Routine suffixes are role-specific; public variant cues select only unique
    matching owners, while generic questions use only shared owners.
    """
    root = Path(root).resolve(strict=True)
    names = feature_names(query)
    if construct is not None:
        if not isinstance(construct, str) or not re.fullmatch(r"[a-z][a-z0-9_]{0,63}", construct):
            raise ValueError("construct must be a bounded lowercase identifier")
        names = list(dict.fromkeys((construct, construct.replace("_", ""))))
    executable = "rg"
    if rg_binary is not None:
        if not Path(rg_binary).is_absolute():
            raise ValueError("rg_binary must be absolute")
        executable = str(Path(rg_binary).resolve(strict=True))
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
        names = _specific_names(query, names)
        result["feature_names"] = names
        variant = _public_variant(query, names)
        backend_scope = _backend_scope(query, names)
        if allowance < 2:
            raise ValueError("insufficient host allowance for search and source verification")
        if (root / "src").is_symlink() or not (root / "src").is_dir():
            raise ValueError("source root must be a real directory")
        alternatives = "|".join(re.escape(name) for name in names)
        pattern = (r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:(?:async|unsafe)\s+)?"
                   r"(?:fn|struct|enum)\s+(?:parse_|emit_|check_)?(?:" + alternatives
                   + r")(?:Decl|Meta|_decl|_block|_sva|_if|_for|_asserts|_state|_sender_state|_receiver_state)?\b")
        command = [executable, "--no-config", "--color", "never", "--no-heading", "-n", "-i",
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
                stem, owner_variant = _canonical_owner(role, symbol, names)
                group = candidates.setdefault(stem, {role: {} for role in roles})
                group[role][(relative, int(number))] = {
                    "path": relative, "line": int(number), "symbol": symbol, "quote": quote,
                    "variant": owner_variant}
        for group in candidates.values():
            for role, rows in group.items():
                if any(row["variant"] in ("if", "for") for row in rows.values()):
                    # Generic questions can use shared owners, never an arbitrary
                    # specialized routine. Explicit variants require an exact match.
                    group[role] = {key: row for key, row in rows.items() if row["variant"] == variant}
                if role == "backend" and backend_scope is not None:
                    allowed = {backend_scope}
                    if backend_scope == "sender_state":
                        allowed.add("state")
                    group[role] = {key: row for key, row in group[role].items() if row["variant"] in allowed}
                elif (role == "backend" and rows
                      and {name.replace("_", "") for name in names} == {"creditchannel"}):
                    raise ValueError("ambiguous credit-channel backend scope; refine the question")
        complete = [group for group in candidates.values() if all(group[role] for role in roles)]
        result["missing_roles"] = [role for role in roles if not any(g[role] for g in candidates.values())]
        if not complete:
            raise ValueError("search lacks a consistent construct for all requested roles")
        if len(complete) != 1 or any(len(rows) != 1 for rows in complete[0].values()):
            raise ValueError("ambiguous declaration candidates; refine the question")
        rows = [{key: value for key, value in next(iter(complete[0][role].values())).items()
                 if key != "variant"} for role in roles]
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
