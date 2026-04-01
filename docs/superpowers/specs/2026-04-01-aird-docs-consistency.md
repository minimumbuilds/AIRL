# aird docs — Ecosystem Documentation Consistency

**Date:** 2026-04-01
**Status:** Approved design
**Target:** `repos/airlDelivery` (new `docs` subcommand) + all ecosystem repos (manifests)
**Prerequisites:** airlDelivery functional (merged)

## Problem

Documentation across the AIRL ecosystem is inconsistent. Individual project READMEs reference stale versions, wrong build commands, outdated dependency lists, and incorrect statistics. ECOSYSTEM.md in the AIRL repo is hand-maintained and drifts from reality. There is no automated way to detect or fix documentation drift.

## Design Principles

1. **Single source of truth per project: `aird.sexp`.** The package manager manifest already needs project metadata. Extend it slightly to cover documentation-relevant fields. Don't maintain the same information in two places.
2. **Generate the overview, validate the details.** ECOSYSTEM.md is generated from manifests. Per-project READMEs are hand-written but validated against manifests.
3. **Agent-consumable output.** `aird docs --check` produces JSON so workflow agents (Critic, Director) can parse drift reports without text scraping.
4. **ECOSYSTEM.md stays in the AIRL repo.** That's the ecosystem's front door — where developers look first.

## Architecture

```
Per-project aird.sexp          ← source of truth (authoritative metadata)
    ↓ read by
aird docs                      ← generates ECOSYSTEM.md from all manifests
aird docs --check              ← validates READMEs against manifests, JSON output
    ↓ consumed by
Workflow Critic                ← flags drift during code review
Scheduled task                 ← periodic ecosystem-wide consistency check
```

---

## Component 1: Extended aird.sexp Manifest

Every project's `aird.sexp` manifest is extended with documentation-relevant fields. Fields marked (new) don't exist yet; the rest are already in the airlDelivery manifest format.

```clojure
(package
  (name "AIRL_castle")
  (version "0.5.0")
  (description "Kafka client SDK implementing the binary TCP protocol from scratch")
  (language "AIRL")                    ;; (new) "AIRL", "Rust + AIRL", "C + asm"
  (status "functional")                ;; (new) "functional", "stable", "in development", "design complete"
  (category "library")                 ;; (new) "core", "library", "tool", "benchmark", "os"
  (depends ["airline" "CairLI"])
  (build "make all")
  (test "make test")
  (binary "airlift")                   ;; (new) optional — named binary if applicable
  (repo-path "../AIRL_castle"))        ;; relative path from AIRL root
```

### New fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `language` | String | Yes | Primary language(s) |
| `status` | String | Yes | One of: `stable`, `functional`, `in development`, `design complete` |
| `category` | String | Yes | One of: `core`, `library`, `tool`, `benchmark`, `os` |
| `binary` | String | No | Named binary output (e.g., `airlift`, `airlint`, `aird`) |

### Computed fields (not in manifest — derived at generation time)

| Field | Source |
|-------|--------|
| LOC | `wc -l src/*.airl` (or language-appropriate pattern) |
| Commits | `git rev-list --count HEAD` |
| Test count | Parse `airtest` output or count test files |

---

## Component 2: aird docs — ECOSYSTEM.md Generator

New subcommand in airlDelivery: `aird docs`

### Usage

```bash
# Generate ECOSYSTEM.md from all project manifests
aird docs --output ../AIRL/ECOSYSTEM.md

# Dry run — print to stdout without writing
aird docs --dry-run

# Generate with computed stats (runs git/wc in each repo)
aird docs --stats
```

### How it works

1. Scan all known project directories (from airlDelivery's registry or a `repos` config)
2. Read each project's `aird.sexp`
3. Optionally compute LOC, commit count, test count per project
4. Render ECOSYSTEM.md using a template that matches the current structure:
   - Overview section
   - Per-category sections (Core, Libraries, Tools, Benchmarks, OS)
   - Per-project entries with: name, description, stats table, status, dependencies
   - Ecosystem stats summary table
   - Build instructions
5. Write to output path (default: `../AIRL/ECOSYSTEM.md`)

### Template

The generated ECOSYSTEM.md follows the existing structure so the transition is seamless. The format is not configurable — it's a single canonical layout.

---

## Component 3: aird docs --check — README Validator

### Usage

```bash
# Check all projects
aird docs --check

# Check a single project
aird docs --check --project AIRL_castle

# Output formats
aird docs --check --format json     # (default) machine-readable
aird docs --check --format text     # human-readable summary
```

### What it validates

For each project, reads `aird.sexp` and `README.md`, then checks:

| Check | How |
|-------|-----|
| **Version match** | Search README for version string (semver pattern), compare to `aird.sexp` version |
| **Build command match** | Search README for build instructions section, verify `aird.sexp` build command appears |
| **Test command match** | Search README for test instructions, verify `aird.sexp` test command appears |
| **Dependencies mentioned** | For each dep in `aird.sexp` depends, check if README mentions it |
| **Status consistency** | Check if README status description aligns with `aird.sexp` status |
| **Stale stats** | If README contains LOC or commit numbers, compute current values and flag if >10% different |

### JSON output format

```json
{
  "version": 1,
  "timestamp": "2026-04-01T20:00:00Z",
  "projects": [
    {
      "name": "AIRL_castle",
      "path": "../AIRL_castle",
      "issues": [
        {
          "check": "version_match",
          "severity": "error",
          "expected": "0.5.0",
          "found": "0.4.0",
          "location": "README.md:7"
        },
        {
          "check": "stale_stats",
          "severity": "warning",
          "field": "commits",
          "expected": 52,
          "found": 50,
          "location": "README.md:15"
        }
      ]
    },
    {
      "name": "CairLI",
      "path": "../CairLI",
      "issues": []
    }
  ],
  "summary": {
    "total_projects": 15,
    "projects_with_issues": 1,
    "total_issues": 2,
    "errors": 1,
    "warnings": 1
  }
}
```

### Severity levels

- **error** — version mismatch, missing build command, wrong dependencies. Must fix.
- **warning** — stale stats, missing optional mentions. Should fix.

---

## Component 4: Workflow Integration

### Critic integration

The workflow Critic runs `aird docs --check --project {project}` as part of every review. If any `error`-level issues are found, it's a blocking finding. Warnings are advisory.

Add to the Critic's review checklist:
- After code review: run `aird docs --check --project {project}`
- If the PR changes `aird.sexp` (version bump, new dep): verify README was also updated

### version-bump.sh integration

When `scripts/version-bump.sh` bumps a project's version, it should also update `aird.sexp` version field. This is a one-line addition to the existing script.

### Scheduled consistency check

Periodic `aird docs --check` across all repos. Can be run by the Director as a maintenance task or via a cron schedule. Flags drift before it accumulates across multiple PRs.

---

## One-Time Migration (Initial Consistency Effort)

### Step 1: Ensure every project has aird.sexp

Audit all repos. Create `aird.sexp` for any project that doesn't have one. Populate from existing README/ECOSYSTEM.md data.

Projects that need manifests (check each):
- AIRL, AIRL_castle, airl_kafka_cli, AIReqL, airlhttp, airlDelivery
- airline, CairLI, airtools, AIRLchart, AirTraffic
- AIRL_bench, kafka_sdk_bench
- AIRLOS, airshell
- airtest (new)

### Step 2: Implement aird docs and aird docs --check

Add `docs` subcommand to airlDelivery. Two modes: generate and check.

### Step 3: Generate ECOSYSTEM.md

Run `aird docs --stats --output ../AIRL/ECOSYSTEM.md`. Review and commit.

### Step 4: Fix all drift

Run `aird docs --check`. Fix every error and warning across all project READMEs. This is the bulk of the one-time effort — updating stale versions, build commands, dependency lists, and stats in 15 READMEs.

### Step 5: Wire up workflow

- Update Critic agent instructions to include `aird docs --check`
- Update `version-bump.sh` to touch `aird.sexp`
- Add periodic check to Director's maintenance tasks

---

## Files Changed

| Location | Change |
|----------|--------|
| `repos/airlDelivery/src/docs.airl` | New module — docs generation and validation |
| `repos/airlDelivery/src/main.airl` | Add `docs` subcommand to CLI |
| `repos/*/aird.sexp` | Add/update manifests (15 projects) |
| `repos/AIRL/ECOSYSTEM.md` | Regenerated from manifests |
| `repos/*/README.md` | Fix drift (one-time, per project) |
| `airl-workflow/scripts/version-bump.sh` | Update aird.sexp on version bump |
| `airl-workflow/agents/critic.md` | Add docs check to review checklist |
