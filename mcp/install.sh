#!/usr/bin/env bash
set -euo pipefail

# ARCH HDL MCP Server — one-line installer
# Usage:
#   ./mcp/install.sh                    # full repo: builds compiler + MCP server
#   ./mcp/install.sh --mcp-only         # just the MCP server (arch binary must be on PATH or set ARCH_BIN)
#   ARCH_BIN=/path/to/arch ./mcp/install.sh --mcp-only

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
VENV_DIR="$SCRIPT_DIR/.venv"
MCP_ONLY=false

for arg in "$@"; do
    case "$arg" in
        --mcp-only) MCP_ONLY=true ;;
    esac
done

echo "==> ARCH HDL MCP Server Installer"
echo ""

# 1. Find or build the arch binary
if [ -n "${ARCH_BIN:-}" ]; then
    # User provided explicit path
    if [ ! -x "$ARCH_BIN" ]; then
        echo "ERROR: ARCH_BIN=$ARCH_BIN does not exist or is not executable."
        exit 1
    fi
    echo "    Using arch binary: $ARCH_BIN"
elif [ "$MCP_ONLY" = true ]; then
    # --mcp-only: find arch on PATH
    if command -v arch &>/dev/null; then
        ARCH_BIN="$(command -v arch)"
        echo "    Found arch on PATH: $ARCH_BIN"
    else
        echo "ERROR: arch binary not found on PATH."
        echo "       Either install the arch compiler first, or set ARCH_BIN=/path/to/arch"
        exit 1
    fi
else
    # Full install: build from source
    ARCH_BIN="$PROJECT_ROOT/target/release/arch"
    if ! command -v cargo &>/dev/null; then
        echo "ERROR: cargo not found. Install Rust (https://rustup.rs) or use --mcp-only with a pre-built binary."
        exit 1
    fi
    echo "==> Building arch compiler (release)..."
    (cd "$PROJECT_ROOT" && cargo build --release)
    echo "    Built: $ARCH_BIN"
fi

# 2. Check python3
if ! command -v python3 &>/dev/null; then
    echo "ERROR: python3 not found. Please install Python 3.10+."
    exit 1
fi

# 3. Set up Python venv + install deps
echo "==> Setting up Python environment..."
python3 -m venv "$VENV_DIR"
"$VENV_DIR/bin/pip" install --quiet -r "$SCRIPT_DIR/requirements.txt"
echo "    Installed MCP SDK"

# 4. Verify
echo "==> Verifying..."
"$VENV_DIR/bin/python" -c "from mcp.server.fastmcp import FastMCP; print('    MCP SDK: OK')"
"$ARCH_BIN" check --help &>/dev/null && echo "    arch binary: OK"

# 5. Write ARCH_BIN path into a .env file so the server can find it at runtime
echo "ARCH_BIN=$ARCH_BIN" > "$SCRIPT_DIR/.env"
echo "    Saved arch binary path to mcp/.env"

# 6. Print config
PYTHON_PATH="$VENV_DIR/bin/python"
SERVER_PATH="$SCRIPT_DIR/arch_mcp_server.py"

echo ""
echo "==> Installation complete!"
echo ""
echo "Add one of the following to your AI tool config:"
echo ""
echo "── Claude Desktop (~/Library/Application Support/Claude/claude_desktop_config.json) ──"
echo ""
cat <<CEOF
{
  "mcpServers": {
    "arch-hdl": {
      "command": "$PYTHON_PATH",
      "args": ["$SERVER_PATH"]
    }
  }
}
CEOF
echo ""
echo "── Claude Code (.claude/settings.json or ~/.claude/settings.json) ──"
echo ""
cat <<CEOF
{
  "mcpServers": {
    "arch-hdl": {
      "command": "$PYTHON_PATH",
      "args": ["$SERVER_PATH"]
    }
  }
}
CEOF
echo ""
echo "── Quick test ──"
echo "  echo '{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2024-11-05\",\"capabilities\":{},\"clientInfo\":{\"name\":\"test\",\"version\":\"0.1\"}}}' | $PYTHON_PATH $SERVER_PATH"
echo ""
