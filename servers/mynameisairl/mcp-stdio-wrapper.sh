#!/bin/sh
# mcp-stdio-wrapper.sh — Line-at-a-time stdin adapter for batch MCP servers
#
# AirTraffic's serve-batch reads all stdin before processing (deadlocks with
# interactive MCP clients). This wrapper feeds one line at a time, collecting
# each response before reading the next request.

while IFS= read -r line; do
    [ -z "$line" ] && continue
    response=$(printf '%s\n' "$line" | /usr/local/bin/mynameisairl 2>/dev/null)
    [ -n "$response" ] && printf '%s\n' "$response"
done
