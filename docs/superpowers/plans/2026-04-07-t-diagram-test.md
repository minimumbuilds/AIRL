# T-Diagram Bootstrap Verification Test Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Write `scripts/t-diagram.sh` — a shell script that compiles the bootstrap chain with the existing `./g3` binary (stage 2) and verifies it produces a sha256-identical binary to `./g3` itself (stage 1).

**Architecture:** Single standalone bash script. Stage 1 = existing `./g3` symlink. Stage 2 = `./g3 -- <bootstrap files> -o /tmp/g3-v2`. Compare sha256 of both. Exit 0 on match, exit 1 on mismatch.

**Tech Stack:** bash, sha256sum, `./g3` CLI

---

### Task 1: Write and run `scripts/t-diagram.sh`

**Files:**
- Create: `scripts/t-diagram.sh`

- [ ] **Step 1: Write the script**

Create `scripts/t-diagram.sh` with these exact contents:

```bash
#!/bin/bash
# T-Diagram bootstrap verification.
# Proves that g3 (stage 1, compiled by airl-driver) and a binary produced by
# g3 compiling itself (stage 2) are sha256-identical.
#
# Usage:
#   bash scripts/t-diagram.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
AIRL_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
export AIRL_STDLIB="${AIRL_STDLIB:-$AIRL_ROOT/stdlib}"

STAGE1="$AIRL_ROOT/g3"
STAGE2="/tmp/g3-v2"

# ── Pre-flight ───────────────────────────────────────────────────────────────

if [ ! -x "$STAGE1" ]; then
    echo "[t-diagram] error: ./g3 not found or not executable — run bash scripts/build-g3.sh first" >&2
    exit 1
fi

for f in bootstrap/lexer.airl bootstrap/parser.airl bootstrap/bc_compiler.airl bootstrap/g3_compiler.airl; do
    if [ ! -f "$AIRL_ROOT/$f" ]; then
        echo "[t-diagram] error: missing bootstrap file: $f" >&2
        exit 1
    fi
done

cd "$AIRL_ROOT"

HASH1=$(sha256sum "$STAGE1" | awk '{print $1}')
SIZE1=$(ls -lh "$STAGE1" | awk '{print $5}')
echo "[t-diagram] stage 1: $HASH1  $STAGE1 ($SIZE1)"

# ── Stage 2: g3 compiles itself ──────────────────────────────────────────────

echo "[t-diagram] running stage 2 (g3 compiles bootstrap chain)..."
"$STAGE1" -- \
    bootstrap/lexer.airl \
    bootstrap/parser.airl \
    bootstrap/bc_compiler.airl \
    bootstrap/g3_compiler.airl \
    -o "$STAGE2"

HASH2=$(sha256sum "$STAGE2" | awk '{print $1}')
SIZE2=$(ls -lh "$STAGE2" | awk '{print $5}')
echo "[t-diagram] stage 2: $HASH2  $STAGE2 ($SIZE2)"

# ── Compare ──────────────────────────────────────────────────────────────────

if [ "$HASH1" = "$HASH2" ]; then
    echo "[t-diagram] PASS: stage 1 == stage 2 ($HASH1)"
    rm -f "$STAGE2"
    exit 0
else
    echo "[t-diagram] FAIL: binaries differ"
    echo "[t-diagram]   stage 1: $HASH1 ($SIZE1)"
    echo "[t-diagram]   stage 2: $HASH2 ($SIZE2)"
    echo "[t-diagram]   stage 2 binary left at $STAGE2 for inspection"
    exit 1
fi
```

- [ ] **Step 2: Make it executable**

```bash
chmod +x scripts/t-diagram.sh
```

- [ ] **Step 3: Run it**

```bash
bash scripts/t-diagram.sh
```

Expected output:
```
[t-diagram] stage 1: <hash>  ./g3 (<size>)
[t-diagram] running stage 2 (g3 compiles bootstrap chain)...
[g3] 312 total -> /tmp/g3-v2
[g3] done
[t-diagram] stage 2: <hash>  /tmp/g3-v2 (<size>)
[t-diagram] PASS: stage 1 == stage 2 (<hash>)
```

- [ ] **Step 4: Commit**

```bash
git add scripts/t-diagram.sh
git commit -m "feat(scripts): add T-diagram bootstrap verification test

Compiles the bootstrap chain with ./g3 (stage 2) and verifies it
produces a sha256-identical binary to ./g3 itself (stage 1) — the
classic self-hosting correctness proof.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```
