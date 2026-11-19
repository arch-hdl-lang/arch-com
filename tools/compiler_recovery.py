"""One-shot host-controlled recovery, with no model-supplied commands or paths."""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import re
import threading

if __package__:
    from .compiler_response import _roles, _unique
    from .compiler_search import retrieval_budget, retrieve_search_context
else:
    from compiler_response import _roles, _unique
    from compiler_search import retrieval_budget, retrieve_search_context

POLICY = "compiler-typed-recovery-v1"
MAX_EXECUTABLE_BYTES = 32 * 1024 * 1024


class RecoverySession:
    """Trusted caller owns the ledger and must include all earlier retrieval work.

    A session permits one recovery request, including invalid requests. No model
    calls, retries, shell commands, graph refreshes or default changes occur here.
    Validation/integrity reads are accounted separately from retrieval operations.
    """

    def __init__(self, root: Path, initial_context: dict, *, rg_binary: Path,
                 total_operations: int = 6, roles=("ast", "parser", "backend")):
        self.root = Path(root).resolve(strict=True)
        self.roles = _roles(roles)
        self.total = total_operations
        if not isinstance(initial_context, dict):
            raise ValueError("initial context must contain trusted retrieval counters")
        self.used = retrieval_budget(initial_context, total_operations=total_operations,
                                     reserve_operations=0)["host_operations"]
        if self.used > self.total:
            raise ValueError("initial retrieval budget already exceeded")
        if not Path(rg_binary).is_absolute():
            raise ValueError("rg_binary must be an explicit absolute executable")
        self.binary = Path(rg_binary).resolve(strict=True)
        self.attempted = False
        self._lock = threading.Lock()
        self.executable_read_calls = 0
        self.executable_read_bytes = 0
        self.binary_hash = self._binary_digest()

    def _binary_digest(self):
        if self.binary.is_symlink() or not self.binary.is_file() or not os.access(self.binary, os.X_OK):
            raise ValueError("rg binary is not executable")
        self.executable_read_calls += 1
        with self.binary.open("rb") as stream:
            data = stream.read(MAX_EXECUTABLE_BYTES + 1)
        self.executable_read_bytes += len(data)
        if len(data) > MAX_EXECUTABLE_BYTES:
            raise ValueError("rg binary exceeds integrity-read bound")
        return hashlib.sha256(data).hexdigest()

    def prepare(self, query: str) -> dict:
        if not isinstance(query, str) or not query.strip() or len(query.encode()) > 8192:
            raise ValueError("query must contain 1..8192 UTF-8 bytes")
        result = {"policy": POLICY, "status": "needs_retrieval", "reason": None}
        if self.attempted or self.total - self.used < 2:
            result["reason"] = "recovery exhausted or insufficient remaining budget"
            return result
        schema = {"type": "object", "additionalProperties": False,
                  "properties": {"construct": {"anyOf": [
                      {"type": "string", "pattern": "^[a-z][a-z0-9_]{0,63}$"}, {"type": "null"}]}},
                  "required": ["construct"]}
        prompt = (query + "\nWithout tools, identify the single ARCH language construct keyword needed for "
                  "compiler ownership retrieval. Return only JSON with construct set to a lowercase "
                  "identifier, or null if uncertain or multiple constructs are requested. "
                  "Do not provide source paths, symbols, citations, commands or explanations. "
                  "The host will search and verify source within its remaining retrieval budget.")
        result.update(status="ready", prompt=prompt, output_schema=schema)
        return result

    def recover(self, answer: str, *, used_tools: bool = False) -> dict:
        result = {"policy": POLICY, "status": "needs_retrieval", "reason": None, "context": None,
                  "rg_binary": str(self.binary), "rg_sha256": self.binary_hash}
        try:
            with self._lock:
                if self.attempted:
                    raise ValueError("recovery already attempted")
                self.attempted = True
            if used_tools or not isinstance(answer, str) or len(answer.encode()) > 256:
                raise ValueError("tool-assisted or oversized recovery request")
            request = json.loads(answer, object_pairs_hook=_unique)
            if (not isinstance(request, dict) or set(request) != {"construct"}
                    or not isinstance(request["construct"], str)
                    or not re.fullmatch(r"[a-z][a-z0-9_]{0,63}", request["construct"])):
                raise ValueError("request must name one construct identifier")
            if self.total - self.used < 2:
                raise ValueError("insufficient remaining retrieval budget")
            if self._binary_digest() != self.binary_hash:
                raise ValueError("rg executable changed before retrieval")
            context = retrieve_search_context(self.root, "Compiler ownership", roles=self.roles,
                                              construct=request["construct"], rg_binary=self.binary,
                                              total_operations=self.total - self.used, reserve_operations=0)
            self.used += context["source_read_calls"] + len(context["commands"])
            result["context"] = context
            if context["status"] != "ready":
                raise ValueError(context["reason"])
            result["status"] = "ready"
        except (OSError, ValueError, TypeError, RuntimeError) as error:
            result["reason"] = str(error)
        result.update(host_operations=self.used, remaining_operations=max(0, self.total - self.used),
                      executable_read_calls=self.executable_read_calls,
                      executable_read_bytes=self.executable_read_bytes)
        return result
