# ARCH HDL MCP Server

An [MCP](https://modelcontextprotocol.io/) server that gives any AI assistant the ability to read the ARCH language specification and invoke the compiler.

## Setup

```bash
# 1. Build the compiler
cargo build --release

# 2. Install Python dependencies
cd mcp
python3 -m venv .venv
.venv/bin/pip install -r requirements.txt
```

## Usage with Claude Desktop

Add to your `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "arch-hdl": {
      "command": "/path/to/arch-com/mcp/.venv/bin/python",
      "args": ["/path/to/arch-com/mcp/arch_mcp_server.py"]
    }
  }
}
```

## Usage with Claude Code

Add to `.claude/settings.json`:

```json
{
  "mcpServers": {
    "arch-hdl": {
      "command": "/path/to/arch-com/mcp/.venv/bin/python",
      "args": ["/path/to/arch-com/mcp/arch_mcp_server.py"]
    }
  }
}
```

## Usage with Codex CLI / VSCode

Codex uses its own MCP registry in `~/.codex/config.toml`. The repo-local
`.mcp.json` file is not sufficient on its own for Codex sessions.

Register the server once with:

```bash
codex mcp add arch-hdl \
  --env ARCH_BIN=/path/to/arch-com/target/release/arch \
  -- /path/to/arch-com/mcp/.venv/bin/python3 \
     /path/to/arch-com/mcp/arch_mcp_server.py
```

Verify it is registered:

```bash
codex mcp list
codex mcp get arch-hdl
```

After registering, restart the Codex session / VSCode extension so the new
session loads the server and exposes the MCP resources and tools.

## Available Resources

| Resource | Description |
|----------|-------------|
| `arch://reference-card` | Full ARCH HDL AI Reference Card — language syntax and examples |
| `arch://specification` | Full ARCH HDL Language Specification |
| `arch://compiler-status` | Current compiler feature status and changelog |
| `arch://doc-comments` | V1 spec for `///` / `//!` doc comments and the `//! ---` YAML frontmatter that captures spec→RTL provenance |

## Available Tools

| Tool | Description |
|------|-------------|
| `arch_check` | Type-check .arch files |
| `arch_build` | Compile .arch to SystemVerilog (returns generated SV) |
| `arch_sim` | Compile + simulate with optional C++ testbench |
| `read_arch_file` | Read a .arch file from the project |
| `write_arch_file` | Write a .arch file to the project |
| `list_arch_files` | List all .arch files in a directory |

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `ARCH_BIN` | `target/release/arch` | Path to the `arch` compiler binary |

## Example Workflow

An AI assistant using this MCP server can:

1. Read `arch://reference-card` to learn the ARCH language
2. Call `get_construct_syntax("doc_comments")` to learn the spec-provenance shape
3. Use `write_arch_file` (or `write_and_check`) to create a design from natural language — capture the user's design spec in front matter (`//! ---`) + per-construct `///` blocks per the directive in `instructions.md`
4. Use `arch_check` to validate — fix errors from diagnostics
5. Use `arch_build_and_lint` to emit SystemVerilog and verify with Verilator
6. Use `arch_sim` to run simulation with a testbench

Spec preservation: when editing an existing `.arch` file, agents are
instructed to preserve all `///`, `//!`, and front-matter content
unless the user explicitly asks to change it.
