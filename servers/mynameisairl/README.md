# mynameisAIRL

MCP prompt server that teaches AIRL to LLMs. Serves the AIRL Language Guide
(`AIRL-LLM-Guide.md`) as an MCP prompt called `teach_airl`.

Built on the [AirTraffic](https://github.com/jbarnes/AirTraffic) MCP framework.

## Building natively

Requires the g3 compiler and the AirTraffic source:

```bash
# Build
AIRL_STDLIB=~/repos/AIRL/stdlib bash build.sh ./mynameisairl

# Or specify paths explicitly
G3=~/repos/AIRL/g3 AT_ROOT=~/repos/AirTraffic bash build.sh ./mynameisairl
```

## Building with Docker

```bash
bash docker-build.sh
```

This builds a minimal container with the server binary and the guide baked in.

## Running

### Native

```bash
# Pass the guide path via CLI
./mynameisairl --guide ~/repos/AIRL/AIRL-LLM-Guide.md

# Or via environment variable
AIRL_GUIDE=~/repos/AIRL/AIRL-LLM-Guide.md ./mynameisairl
```

### Docker

```bash
docker run -i mynameisairl
```

The Docker image includes the guide at `/data/AIRL-LLM-Guide.md` (the default path).

## Installing as an MCP server in Claude Code

```bash
claude mcp add mynameisairl -- ./mynameisairl --guide ~/repos/AIRL/AIRL-LLM-Guide.md
```

Or with Docker:

```bash
claude mcp add mynameisairl -- docker run -i mynameisairl
```

## Verifying

After installation, ask Claude to use the `teach_airl` prompt. It should return
the full AIRL language guide content.

You can also test manually by piping JSON-RPC messages:

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | \
  ./mynameisairl --guide ~/repos/AIRL/AIRL-LLM-Guide.md
```

## Testing

```bash
bash tests/test-mynameisairl.sh
```

## Architecture

mynameisAIRL is a prompt-only MCP server (no tools). It:

1. Parses CLI args for `--guide` (path to the AIRL guide)
2. Falls back to `AIRL_GUIDE` env var, then `/data/AIRL-LLM-Guide.md`
3. Reads the guide file at startup
4. Registers a single prompt `teach_airl` with the guide as message content
5. Serves via AirTraffic's batch stdio transport
