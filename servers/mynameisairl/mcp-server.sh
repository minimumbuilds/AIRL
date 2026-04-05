#!/usr/bin/env bash
# MCP server wrapper — ensures correct working directory for guide file
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"
exec ./mynameisairl "$@"
