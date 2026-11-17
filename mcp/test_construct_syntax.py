"""Tests for MCP get_construct_syntax tool and Reference Card syntax correctness (#1049).

Ensures construct templates served by MCP match ARCH HDL compiler syntax
and spec grammar, preventing syntax rejection when agents or developers use
`get_construct_syntax()`.

Run with:
    python -m unittest discover -s mcp -p "test_*.py"
"""

import os
import shutil
import subprocess
import tempfile
import unittest

import arch_mcp_server as server


class ConstructSyntaxTests(unittest.TestCase):
    def test_ram_syntax_uses_port_groups_and_valid_init(self):
        """Issue #1049: ram must use `ports <name> ... end ports <name>` and valid init."""
        syntax = server.get_construct_syntax("ram")
        self.assertNotIn("[ERROR]", syntax)

        # Port groups must use 'ports', not scalar 'port'
        self.assertIn("ports rd", syntax)
        self.assertIn("end ports rd", syntax)
        self.assertIn("ports wr", syntax)
        self.assertIn("end ports wr", syntax)

        # Must not contain outdated single-line scalar port syntax for port groups
        self.assertNotIn("port rd Clock<D>", syntax)
        self.assertNotIn("end port rd", syntax)
        self.assertNotIn("end port wr", syntax)

        # init must not be 'none' (compiler accepts zero | file(...))
        self.assertNotIn("init: none;", syntax)
        self.assertIn("init: zero;", syntax)

    def test_regfile_syntax_uses_ports_keyword(self):
        """Regfile multi-port declarations must use `ports[N] <name> ... end ports <name>`."""
        syntax = server.get_construct_syntax("regfile")
        self.assertNotIn("[ERROR]", syntax)

        self.assertIn("ports[1] rd0", syntax)
        self.assertIn("end ports rd0", syntax)
        self.assertIn("ports[1] wr0", syntax)
        self.assertIn("end ports wr0", syntax)
        self.assertNotIn("port rd0", syntax)
        self.assertNotIn("end port rd0", syntax)

    def test_counter_syntax_attributes_ordering(self):
        """Counter attributes (kind, direction) must precede params and ports."""
        syntax = server.get_construct_syntax("counter")
        self.assertNotIn("[ERROR]", syntax)

        kind_idx = syntax.find("kind wrap;")
        dir_idx = syntax.find("direction: up;")
        param_idx = syntax.find("param T:")
        port_idx = syntax.find("port clk:")

        self.assertNotEqual(kind_idx, -1)
        self.assertNotEqual(dir_idx, -1)
        self.assertNotEqual(param_idx, -1)
        self.assertNotEqual(port_idx, -1)

        self.assertLess(kind_idx, param_idx)
        self.assertLess(dir_idx, param_idx)
        self.assertLess(param_idx, port_idx)

    def test_arbiter_syntax_attributes_ordering(self):
        """Arbiter attributes (policy, latency) must precede params and ports."""
        syntax = server.get_construct_syntax("arbiter")
        self.assertNotIn("[ERROR]", syntax)

        policy_idx = syntax.find("policy round_robin;")
        latency_idx = syntax.find("latency 1;")
        param_idx = syntax.find("param N:")
        port_idx = syntax.find("port clk:")

        self.assertNotEqual(policy_idx, -1)
        self.assertNotEqual(latency_idx, -1)
        self.assertNotEqual(param_idx, -1)
        self.assertNotEqual(port_idx, -1)

        self.assertLess(policy_idx, param_idx)
        self.assertLess(latency_idx, param_idx)
        self.assertLess(param_idx, port_idx)

    def test_fifo_syntax_kind_and_latency(self):
        """FIFO template must specify valid kind and latency before ports."""
        syntax = server.get_construct_syntax("fifo")
        self.assertNotIn("[ERROR]", syntax)

        kind_idx = syntax.find("kind fifo;")
        latency_idx = syntax.find("latency 1;")
        port_idx = syntax.find("port clk:")

        self.assertNotEqual(kind_idx, -1)
        self.assertNotEqual(latency_idx, -1)
        self.assertNotEqual(port_idx, -1)

        self.assertLess(kind_idx, port_idx)
        self.assertLess(latency_idx, port_idx)

    def test_compiler_check_construct_templates(self):
        """If `arch` binary is available, test that key construct templates pass `arch check`."""
        arch_bin = shutil.which(server.ARCH_BIN) or shutil.which("arch")
        if not arch_bin:
            # Check target/debug or target/release
            for candidate in ["target/debug/arch.exe", "target/debug/arch", "target/release/arch.exe", "target/release/arch"]:
                p = os.path.join(server.PROJECT_ROOT, candidate)
                if os.path.isfile(p):
                    arch_bin = p
                    break

        if not arch_bin:
            self.skipTest("ARCH binary not found; skipping subprocess check.")

        test_cases = [
            ("ram", """
domain SysDomain
  freq_mhz: 100
end domain SysDomain

ram TestRam
  kind simple_dual;
  latency 1;
  param DEPTH: const = 1024;
  port clk: in Clock<SysDomain>;

  store
    weights: Vec<SInt<8>,  DEPTH>;
  end store

  ports rd
    en:   in Bool;
    addr: in UInt<10>;
    data: out SInt<8>;
  end ports rd

  ports wr
    en:   in Bool;
    addr: in UInt<10>;
    data: in SInt<8>;
  end ports wr

  init: zero;
end ram TestRam
"""),
            ("regfile", """
domain SysDomain
  freq_mhz: 100
end domain SysDomain

regfile TestRegFile
  param DEPTH: const = 32;
  param T: const = 32;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;

  ports[1] rd0
    addr: in UInt<5>;
    data: out UInt<T>;
  end ports rd0

  ports[1] wr0
    en:   in Bool;
    addr: in UInt<5>;
    data: in UInt<T>;
  end ports wr0

  kind flop;
  forward write_before_read: false;
end regfile TestRegFile
"""),
            ("fifo", """
domain SysDomain
  freq_mhz: 100
end domain SysDomain

fifo TestFifo
  kind fifo;
  param DEPTH: const = 64;
  param T: type = UInt<32>;
  latency 1;

  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port push_valid: in Bool;
  port push_ready: out Bool;
  port push_data:  in T;
  port pop_valid:  out Bool;
  port pop_ready:  in Bool;
  port pop_data:   out T;
end fifo TestFifo
"""),
            ("counter", """
domain SysDomain
  freq_mhz: 100
end domain SysDomain

counter TestCounter
  kind wrap;
  direction: up;
  param T: const = 8;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port en: in Bool;
  port count: out UInt<T>;
end counter TestCounter
"""),
            ("arbiter", """
domain SysDomain
  freq_mhz: 100
end domain SysDomain

arbiter TestArbiter
  policy round_robin;
  latency 1;
  param N: const = 4;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  ports[N] req
    valid: in Bool;
    ready: out Bool;
  end ports req
  port grant_valid: out Bool;
  port grant_requester: out UInt<2>;
end arbiter TestArbiter
"""),
        ]

        with tempfile.TemporaryDirectory() as tmpdir:
            for name, code in test_cases:
                file_path = os.path.join(tmpdir, f"test_{name}.arch")
                with open(file_path, "w", encoding="utf-8") as f:
                    f.write(code.strip())

                proc = subprocess.run(
                    [arch_bin, "check", file_path],
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(
                    proc.returncode,
                    0,
                    f"Template check failed for {name}:\nSTDOUT:\n{proc.stdout}\nSTDERR:\n{proc.stderr}",
                )


if __name__ == "__main__":
    unittest.main()
