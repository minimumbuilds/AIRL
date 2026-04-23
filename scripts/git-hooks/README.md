# Tracked git hooks

Hooks in this directory are activated via git's `core.hooksPath` config —
not by copying into `.git/hooks/`. Run once per fresh clone:

```bash
bash scripts/git-hooks/install.sh
```

After that, git runs these hooks on push / commit / etc. Edits to the
tracked files take effect immediately on the next invocation; no
re-install needed.

## Hooks

- **`pre-push`** — enforces fixpoint verification before pushing to
  `main`/`master`. Runs the full 3-stage build (`cargo build` →
  `scripts/build-g3.sh` → `tests/fixpoint/fixpoint_smoke.airl`) inside
  `rust:slim-bullseye` Docker so the GLIBC ABI is consistent across all
  stages. Requires 8 GB Docker memory (`G3_BUILD_MEM=8g` is set by the
  hook) since the self-hosted bootstrap peaks at ~5.9 GB RSS while
  compiling `bootstrap/linearity.airl`.

  Triggers only when the push touches `bootstrap/`, `src/`, `crates/`,
  or `stdlib/`. No-op for doc-only or spec-only pushes.

  Explicit opt-out: `git push --no-verify`.

## Revert to default

```bash
git config --unset core.hooksPath
```
