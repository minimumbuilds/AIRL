# Multi-Agent Workflow Implementation Design

**Date:** 2026-03-29
**Status:** Draft
**Source:** `airl_multiagent_spec_v0.4.docx`

## Context

The AIRL project ecosystem spans 5 repositories (AIRL, CairLI, airline, AIRL_castle, airl_kafka_cli). Development work across these repos currently lacks formal coordination — there is no structured review gate, no cross-repo dependency tracking, and no enforcement of the g3 compiler constraint. The multi-agent workflow spec (v0.4) defines five agent roles that govern development work: Supervisor, Worker, Approver, Critic, and Merger. This design implements that spec as a Claude Code team-based orchestration system backed by SQLite state and shell scripts.

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Agent realization | Claude Code teams | Persistent named identities, inbox-based messaging, survives session boundaries |
| State management | SQLite | Structured queries, transactional, spec-faithful (§6) |
| Infrastructure location | `repos/airl-workflow` | Dedicated repo, clean separation from project repos |
| Orchestration method | Shell scripts + AGENTS.md | Debuggable, testable, inspectable; scripts wrap sqlite3 + file generation |
| Delivery strategy | Vertical slices | Each increment delivers working end-to-end capability |

## Repository Structure

```
repos/airl-workflow/
├── CLAUDE.md
├── AGENTS.md                    # Team definition (Supervisor as leader)
├── db/
│   ├── schema.sql
│   └── workflow.db              # (gitignored)
├── agents/
│   ├── supervisor.md
│   ├── worker.md
│   ├── approver.md
│   ├── critic.md
│   └── merger.md
├── scripts/
│   ├── init-db.sh
│   ├── dispatch.sh
│   ├── submit.sh
│   ├── review.sh
│   ├── approve.sh
│   ├── merge.sh
│   ├── escalate.sh
│   ├── req-spec.sh
│   └── query.sh
├── templates/
│   ├── approval-spec.md
│   ├── requirement-spec.md
│   ├── fix-request.md
│   └── escalation-report.md
├── artifacts/                   # (gitignored except templates)
│   ├── approvals/
│   ├── req-specs/
│   ├── fix-requests/
│   └── escalations/
└── tests/
    └── test-workflow.sh
```

## SQLite Schema

Maps directly to §6 of the spec. `artifact_path` columns link state rows to markdown artifacts.

```sql
CREATE TABLE agents (
    identity    TEXT PRIMARY KEY,  -- e.g. 'worker:airl:01'
    role        TEXT NOT NULL,     -- supervisor|worker|approver|critic|merger
    project     TEXT,              -- NULL for non-project roles
    status      TEXT NOT NULL DEFAULT 'idle',  -- idle|active|blocked
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE issue_queue (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    project     TEXT NOT NULL,
    issue_ref   TEXT NOT NULL UNIQUE,
    title       TEXT NOT NULL,
    priority    INTEGER NOT NULL DEFAULT 50,  -- lower = higher; AIRL=10
    status      TEXT NOT NULL DEFAULT 'queued',  -- queued|assigned|completed|cancelled
    assigned_to TEXT REFERENCES agents(identity),
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE worktrees (
    name        TEXT PRIMARY KEY,  -- e.g. 'worker:cairli:01/issue-42'
    repo        TEXT NOT NULL,
    issue_ref   TEXT NOT NULL REFERENCES issue_queue(issue_ref),
    worker_id   TEXT NOT NULL REFERENCES agents(identity),
    status      TEXT NOT NULL DEFAULT 'active',  -- active|submitted|merged|abandoned
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE req_specs (
    spec_id     TEXT PRIMARY KEY,  -- e.g. 'REQ-2025-001'
    requesting_agent TEXT NOT NULL REFERENCES agents(identity),
    source_repo TEXT NOT NULL,
    target_repo TEXT NOT NULL,
    reason      TEXT NOT NULL,
    proposed_change TEXT NOT NULL,
    priority    TEXT NOT NULL DEFAULT 'Normal',  -- Blocking|High|Normal
    status      TEXT NOT NULL DEFAULT 'pending_review',
    -- pending_review|approved|dispatched|completed|rejected
    artifact_path TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE blocked_workers (
    worker_id   TEXT NOT NULL REFERENCES agents(identity),
    req_spec_id TEXT NOT NULL REFERENCES req_specs(spec_id),
    resolved    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (worker_id, req_spec_id)
);

CREATE TABLE approval_specs (
    spec_id     TEXT PRIMARY KEY,  -- e.g. 'APPROVE-2025-001'
    project     TEXT NOT NULL,
    repo        TEXT NOT NULL,
    worker_id   TEXT NOT NULL REFERENCES agents(identity),
    issue_ref   TEXT NOT NULL,
    worktree    TEXT NOT NULL REFERENCES worktrees(name),
    summary     TEXT NOT NULL,
    test_results TEXT NOT NULL,
    g3_verified INTEGER,          -- NULL for repos/AIRL
    critic_status TEXT NOT NULL,  -- approved|approved_with_advisory
    merge_status TEXT NOT NULL DEFAULT 'pending',  -- pending|merged|failed
    artifact_path TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE escalations (
    report_id   TEXT PRIMARY KEY,
    submission_type TEXT NOT NULL, -- code_change|req_spec
    worker_id   TEXT NOT NULL,
    issue_ref   TEXT NOT NULL,
    reason      TEXT NOT NULL,     -- critic_block|extensive_rework|cross_repo|rejection
    critic_category TEXT,
    recommended_action TEXT,       -- re_scope|cancel|re_assign|user_decision
    resolution_status TEXT NOT NULL DEFAULT 'open',
    artifact_path TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    resolved_at TEXT
);

CREATE TABLE compiler_checks (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    worker_id   TEXT NOT NULL REFERENCES agents(identity),
    project     TEXT NOT NULL,
    verified    INTEGER NOT NULL,
    checked_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
```

## Agent Role Definitions

### Supervisor (`supervisor.md`)
- **Identity:** `supervisor:workflow:01`
- **Team role:** Leader
- **Reads:** `issue_queue`, `blocked_workers`, `escalations`, `compiler_checks`
- **Writes:** `issue_queue` (assign), `agents` (status), `compiler_checks`
- **Scripts:** `dispatch.sh`, `query.sh`, `init-db.sh`
- **Key rules:**
  - Prioritizes repos/AIRL in dispatch queue (priority 10 vs default 50)
  - Verifies g3 compiler config before dispatching non-AIRL Workers
  - Sends unblock signals when blocked Workers' dependencies resolve
  - Presents escalations to user with summary + recommended action

### Worker (`worker.md`)
- **Identity template:** `worker:{project}:{instance}`
- **Team role:** Member (one active per project)
- **Reads:** own worktree state, issue details
- **Writes:** `worktrees` (create), `req_specs` (create)
- **Scripts:** `submit.sh`, `req-spec.sh`, `query.sh`
- **Key rules:**
  - Only works in assigned repo
  - Creates worktree named `worker:{project}:{instance}/issue-{id}`
  - Only repos/AIRL may invoke cargo or bootstrap build
  - All other repos compile exclusively with g3
  - Updates all documentation before submission
  - May spawn subagents within repo (bound by all Worker rules)
  - Subagents may not produce Requirement Specs or communicate with other roles

### Approver (`approver.md`)
- **Identity:** `approver:workflow:01`
- **Team role:** Member (review gate)
- **Reads:** submissions, Critic output, all state
- **Writes:** `approval_specs`, `escalations`
- **Scripts:** `review.sh`, `approve.sh`, `escalate.sh`, `query.sh`
- **Key rules:**
  - Invokes Critic for every submission (code changes and req-specs)
  - Applies §5.2 checklists (code change + req-spec)
  - Routes: trivial → Worker (Fix Request), blocking → Supervisor (Escalation), clean → Merger (Approval Spec)
  - Verifies g3 compliance on all non-AIRL submissions

### Critic (`critic.md`)
- **Identity:** `critic:workflow:01`
- **Team role:** Member (adversarial reviewer)
- **Reads:** submissions (via Approver), codebase state
- **Writes:** none (output goes to Approver)
- **Scripts:** `query.sh` (read-only)
- **Key rules:**
  - Code change blocking categories (§7.1): Correctness, Security, Ethos Violation, Architectural Regression, Breaking Change, Deferred Problem
  - Req-spec blocking categories (§7.2): Unjustified, Poorly Scoped, Duplicate, Ethos Violation
  - Must name specific category for any Blocking finding
  - Advisory findings do not block
  - Communicates only with Approver — never with Worker, Supervisor, or Merger

### Merger (`merger.md`)
- **Identity:** `merger:workflow:01`
- **Team role:** Member (sole main-branch write access)
- **Reads:** `approval_specs`, `worktrees`
- **Writes:** `approval_specs` (merge_status), `worktrees` (status)
- **Scripts:** `merge.sh`, `query.sh`
- **Key rules:**
  - Only acts on valid, complete Approval Specs
  - Verifies worktree/branch exist and match before merging
  - Squash merge with meaningful commit message (not auto-generated)
  - Runs full test suite post-merge; reports failure to Supervisor if tests fail
  - Deletes worktree after successful merge
  - No informal or verbal merge requests accepted

## Communication Surface

```
Supervisor ──dispatch──► Worker
Supervisor ──unblock──► Worker
Worker ──submit──► Approver
Worker ──req-spec──► Approver
Approver ──review-request──► Critic
Critic ──analysis──► Approver
Approver ──fix-request──► Worker
Approver ──approval-spec──► Merger
Approver ──escalation──► Supervisor
Approver ──approved-req-spec──► Supervisor
Merger ──completion/failure──► Supervisor
Supervisor ──escalation──► User
```

## Vertical Slices

### Slice 1: Foundation + Dispatch-Execute-Merge
**Goal:** End-to-end happy path without review gate.

- Initialize `repos/airl-workflow` with CLAUDE.md, AGENTS.md
- SQLite schema + `init-db.sh`
- `agents/supervisor.md`, `agents/worker.md`, `agents/merger.md`
- `scripts/dispatch.sh` — issue → queue → assign Worker
- `scripts/submit.sh` — Worker marks done, notifies Merger directly
- `scripts/merge.sh` — squash merge, test, update state
- `scripts/query.sh` — basic state queries
- **Test:** dispatch issue to repos/AIRL → Worker creates worktree → trivial change → Merger merges

### Slice 2: Approver + Critic Review Gate
**Goal:** Full review pipeline between Worker and Merger.

- `agents/approver.md`, `agents/critic.md`
- `scripts/submit.sh` updated → Worker submits to Approver
- `scripts/review.sh` — Approver invokes Critic
- `scripts/approve.sh` — issues Approval Spec → Merger
- Fix Request flow for trivial issues
- Blocking categories enforced in Critic AGENTS.md
- `templates/approval-spec.md`, `templates/fix-request.md`
- **Test:** submission → Critic review → approval → merge

### Slice 3: Escalation Path
**Goal:** Blocking findings and rework route to Supervisor → User.

- `scripts/escalate.sh` — Escalation Report creation
- `templates/escalation-report.md`
- Escalation decision matrix (§9) in Supervisor AGENTS.md
- **Test:** blocking submission → escalation → user decision

### Slice 4: Cross-Repo Requirement Specs
**Goal:** Workers can request changes in other repos via formal specs.

- `scripts/req-spec.sh` — create Requirement Spec
- `templates/requirement-spec.md`
- Req-spec blocking categories in Critic AGENTS.md
- Req-spec approval flow: Approver → Supervisor → target Worker
- Blocked worker tracking + unblock signals
- **Test:** CairLI Worker needs AIRL change → req-spec → review → dispatch

### Slice 5: g3 Compiler Enforcement + Multi-Project Queue
**Goal:** Full g3 compliance and priority-based dispatch.

- g3 verification at Supervisor dispatch (three-layer enforcement)
- Three-stage build protocol for AIRL Workers (§2.3)
- Priority weighting (AIRL=10, others=50)
- Per-project Worker AGENTS.md variants
- **Test:** CairLI dispatch verifies g3; AIRL dispatch requires 3-stage build

### Slice 6: Document Formats + Hardening
**Goal:** Full spec fidelity and edge case coverage.

- All §8 templates finalized field-for-field
- §5.2 checklists mechanically enforced
- Merger post-merge test failure handling (§4.5)
- Trivial fix path for both submission types (§4.2)
- Complete state transition logging
- **Test:** end-to-end integration covering all paths

## Verification

Each slice has its own test, plus:
- **Unit:** each script tested in isolation with mock SQLite state
- **Integration:** `tests/test-workflow.sh` runs full happy path + escalation path
- **Manual:** dispatch a real issue to repos/AIRL, verify the full Supervisor → Worker → Approver → Critic → Merger flow works end-to-end with Claude Code teams
