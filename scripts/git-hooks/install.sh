#!/usr/bin/env bash
# Point this clone's git at the tracked hooks in scripts/git-hooks/.
# Run once per fresh clone:
#   bash scripts/git-hooks/install.sh
#
# This uses git's native core.hooksPath setting (git 2.9+). No symlinks, no
# per-hook plumbing — adding a new hook file to scripts/git-hooks/ makes it
# active on the next push/commit/etc.
#
# To revert to the default .git/hooks/ location:
#   git config --unset core.hooksPath
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

HOOKS_DIR="scripts/git-hooks"

if [[ ! -d "$HOOKS_DIR" ]]; then
    echo "error: $HOOKS_DIR does not exist in this repo" >&2
    exit 1
fi

# Ensure every hook file is executable (copying from .git/hooks/ sometimes
# loses the +x bit on filesystems that don't track it).
find "$HOOKS_DIR" -type f ! -name "*.md" ! -name "install.sh" -exec chmod +x {} +

git config core.hooksPath "$HOOKS_DIR"
echo "[install-hooks] core.hooksPath -> $HOOKS_DIR"
echo "[install-hooks] Active hooks:"
for f in "$HOOKS_DIR"/*; do
    name="$(basename "$f")"
    case "$name" in
        install.sh|*.md) continue ;;
    esac
    echo "  - $name"
done
