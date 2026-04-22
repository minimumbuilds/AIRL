# AirTraffic Unified Tool Registration + Workflow Deconflation

**Date:** 2026-04-22
**Status:** Design approved, ready for implementation plan
**Scope:** AirTraffic library (`~/repos/AirTraffic`) + mynameisAIRL
(only current consumer)

## Problem 1 — Registration / dispatch drift

AirTraffic's MCP tool registration and tool dispatch are structurally
independent:

- `airtraffic-tool server tool-def` adds a schema to `server["tools"]`.
- `airtraffic-serve-batch server handler` takes a single dispatch
  function `(fn [tool-name args] ...)` that the server author writes as
  a `(cond ...)` or name-matching if-chain.

Nothing ties the two together. An author can:

1. Register a schema but forget the dispatch branch → `tools/list`
   advertises the tool; `tools/call` fails at runtime.
2. Add a dispatch branch but forget to register → handler is dead code
   because MCP clients don't know the tool exists.
3. Typo one but not the other → silent drift.

We hit case 1 on 2026-04-22 when mynameisAIRL's entry point called
`airtraffic-prompt` but no `airtraffic-tool` — 12 handlers existed, zero
surfaced to MCP clients.

## Problem 2 — Workflow taxonomy conflated into the framework

`airtraffic.airl:9-44` hardcodes a role/tool allowlist that belongs to
a specific application domain (the airl-workflow orchestration system)
inside what the top-level ecosystem README explicitly labels the
generic MCP framework:

```airl
(defn airtraffic-role-tools
  :body (if (= role "supervisor")
    ["workflow_dispatch" "workflow_unblock" "workflow_query"
     "workflow_check_g3_staleness" "workflow_check_stale"]
    (if (= role "worker")
      ["workflow_submit" "workflow_req_spec" "workflow_query"] ...)))

(defn airtraffic-valid-role
  :body (list-contains? ["supervisor" "worker" "approver" "critic" "merger"] role))
```

The ATC orchestration server (the actual home of these roles)
lives at `~/repos/airl-workflow/atc/` and is implemented in Python
with an entirely different tool-name schema (`dispatch_issue`,
`claim_issue`, `intake_issue`, `submit`, `approve`, `escalate` — surfaced
as the `mcp__atc__*` tool family). The AIRL taxonomy hardcoded in
AirTraffic references tool names that no current consumer implements.
Verified by grep on 2026-04-22: no AIRL or Python file in
`~/repos/airl-workflow/` (or anywhere else on disk) references
`workflow_dispatch`, `workflow_submit`, or `airtraffic-role-tools`.

The practical consequence: every non-workflow AirTraffic consumer
(mynameisAIRL is the only one we have today) must ship a
`patch-tool-allowed.airl` that uses first-def-wins to override
`airtraffic-tool-allowed`. That patch is load-bearing for the build —
without it, `airtraffic-tool` rejects every `airl_*` name. The patch
is not extensibility; it's shimming around a design mistake.

`worker-id` in `airtraffic-new` is the same story — a workflow-identity
field leaked into the generic server state.

## Why the obvious fix didn't happen sooner (and its real constraint)

`airtraffic.airl:83-84` claims g3 AOT does not support functions as map
values, which would rule out bundling the handler into the tool-def map.
Verified 2026-04-22 — the comment is **partially** right, **fully**
misleading, and we can work around the real constraint:

| Form                                               | g3 AOT |
|---                                                 |---     |
| `(fn [x] ...)` stored in Map, retrieved, called   | ✅     |
| `(fn [x] (f cache x))` (closure) in Map, called   | ✅     |
| Bare named `defn` in Map, retrieved, called        | ❌ `airl_call_closure: not a Closure` |
| Bare named `defn` in List, retrieved, called       | ❌ same |

g3's indirect-call path only recognizes the **Closure** tag. Named
`defn` values have a different runtime tag (suited for static dispatch)
that `airl_call_closure` rejects. But any `(fn [...] ...)` lambda —
whether it captures outer bindings or not — is Closure-tagged and works
through map/list lookup. The stale comment overstated the limitation
and caused the design to avoid fn-in-map entirely when the real
workaround (always wrap) is trivial.

**Design rule:** handlers passed to `airtraffic-tool` MUST be lambdas,
not bare named defns. `airtraffic-tool` could enforce this by wrapping
the handler in `(fn [args] (handler args))` before storing it — but
that wrapper itself invokes a named defn through an indirect call,
which fails for the same reason. The robust approach is to require
callers to pass lambdas. mynameisAIRL's cache-capture pattern already
produces lambdas naturally (`(fn [args] (handle-foo cache args))`),
so no friction there.

Regression gate: `tests/aot/round2_fn_in_map.airl` exercises both
capturing and non-capturing closures in a map, retrieved and called
through g3 — must stay green.

## Design

Two coordinated changes:

**A.** Collapse schema and handler into one map entry; dispatch derives
from registered tools (solves Problem 1 — drift).

**B.** Remove hardcoded workflow taxonomy; make role-based tool gating
pluggable via an optional allowlist supplied at `airtraffic-new` time
(solves Problem 2 — conflation).

They land together because both break `airtraffic-new` / `airtraffic-tool`
signatures. Coordinating them is cheaper than sequencing them.

---

### A1. New `airtraffic-tool` (3-arity)

```airl
(defn airtraffic-tool
  :sig [(server : _) (tool-def : Map) (handler : _) -> Map]
  :requires [(valid server) (valid tool-def)]
  :ensures [(valid result)]
  :body
    (let (tool-name : String (map-get tool-def "name"))
         (role : String (map-get server "role"))
      (if (airtraffic-tool-allowed role tool-name)
        (let (augmented : Map (map-set tool-def "_handler" handler))
             (tools : List (map-get server "tools"))
          (map-set server "tools" (append tools augmented)))
        server)))
```

The handler is stored under `"_handler"`. Underscore-prefix is the
convention for "internal, not serialized" — consistent with how many
JSON-oriented frameworks handle internal state adjacent to wire-format
fields.

### A2. New `airtraffic-tools` (list variant — ergonomic)

```airl
(defn airtraffic-tools
  :sig [(server : _) (bindings : List) -> Map]
  :intent "Register a list of (tool-def, handler) pairs"
  :requires [(valid server) (valid bindings)]
  :ensures [(valid result)]
  :body
    (fold (fn [s pair]
            (airtraffic-tool s (map-get pair "schema") (map-get pair "handler")))
          server
          bindings))
```

Bindings are maps `{"schema" <tool-def>, "handler" <fn>}`. This lets
server authors define a single list-of-tool-bindings function rather
than maintaining a parallel schema list + dispatch if-chain.

### A3. New `airtraffic-serve` (replaces `airtraffic-serve-batch`)

Inlines the existing read loop (from `airtraffic-serve-batch`'s body).
The public API no longer requires the caller to supply a dispatcher —
the library constructs one from `server["tools"]`:

```airl
(defn airtraffic-serve
  :sig [(server : _) -> Unit]
  :requires [(valid server)]
  :body
    (let (auto-dispatcher : _
          (fn [tool-name args]
            (let (tools : List (map-get server "tools"))
                 (found : List (filter
                   (fn [t] (= (map-get t "name") tool-name)) tools))
              (if (= (length found) 0)
                (Err (str "unknown tool: " tool-name))
                (let (h : _ (map-get (head found) "_handler"))
                  (h args))))))
      ;; body of the old airtraffic-serve-batch goes here, using
      ;; auto-dispatcher in place of the passed-in dispatcher.
      ...))
```

The existing `airtraffic-dispatch` / `airtraffic-handle-tools-call`
internals work unchanged — they already accept a dispatcher fn. We just
stop requiring the server author to write one.

### A4. `airtraffic-handle-tools-list` must strip `_handler`

Today the handler returns `(map-get server "tools")` verbatim. With
`_handler` embedded, we'd leak a function value into JSON serialization
(which either crashes or produces garbage). Fix by projecting each tool
to its MCP-visible fields:

```airl
(defn airtraffic-tool-public-view
  :sig [(tool : Map) -> Map]
  :body
    (let (base : Map (map-from
      ["name" (map-get tool "name")
       "description" (map-get-or tool "description" "")
       "inputSchema" (map-get tool "inputSchema")]))
      base))

(defn airtraffic-handle-tools-list
  :sig [(server : _) (id : _) -> String]
  :body
    (let (tools : List (map-get server "tools"))
         (public : List (map airtraffic-tool-public-view tools))
      (jsonrpc-response id (map-from ["tools" public]))))
```

### A5. Deprecation of `airtraffic-serve-batch`

Keep it as a thin shim that registers a no-op tool set and uses the old
batch dispatcher — OR remove it entirely. mynameisAIRL is the only known
consumer, and it's moving to the new API in the same PR. **Decision:
remove it.** Cleanliness > backward-compat for a library with one
consumer.

### A6. Remove stale fn-in-map comment

`airtraffic.airl:83-84` currently reads:

> ;; Note: "content" is a static string. g3 AOT does not support fn-in-map,
> ;; so content-fn is not supported — provide the content string directly.

Replace with a factual note about the current design (static content
only for resources, because per-request generation requires passing
request context, which isn't wired through `airtraffic-handle-resources-read`
today). Do not retain the false claim about AOT.

---

### B1. Delete hardcoded workflow taxonomy

Remove from `airtraffic.airl`:

- `airtraffic-role-tools` (lines 11-28) — entire definition
- `airtraffic-valid-role` (lines 32-36) — entire definition
- The default body of `airtraffic-tool-allowed` that consults
  `airtraffic-role-tools` — it gets a new implementation (B3).

No migration path is needed for the tool-name list because **no file in
`~/repos/` references `workflow_dispatch`, `workflow_submit`, or
`airtraffic-role-tools` outside of AirTraffic itself.** The taxonomy is
referenced only by its own definition. Nothing consumes it.

### B2. `airtraffic-new` takes an optional allowlist

New signature (arity-4 stays, but the 3rd argument changes meaning —
still a breaking change):

```airl
(defn airtraffic-new
  :sig [(name : _) (version : _) (role : _) (allowlist : _) -> Map]
  :requires [(valid name) (valid version) (valid role)]
  :ensures [(valid result)]
  :body (map-from
    ["name" name
     "version" version
     "role" role
     "allowlist" allowlist       ;; nil | (fn [role name] -> Bool)
     "tools" []
     "prompts" []
     "resources" []]))
```

- `role` remains a free-form string. AirTraffic no longer validates it.
  (The field is kept purely so the allowlist closure can see "who is
  asking" — consumers can ignore it and gate purely on tool-name if
  preferred.)
- `allowlist` is either `nil` (treated as "allow everything" — useful
  for prototype servers and for consumers that do their own gating
  elsewhere) or a closure `(fn [role tool-name] -> Bool)`.
- The old `worker-id` field is removed from the server state. It was
  workflow-specific identity that AirTraffic itself never read.

### B3. `airtraffic-tool-allowed` reads from the server, not a global

```airl
(defn airtraffic-tool-allowed
  :sig [(server : _) (tool-name : String) -> Bool]
  :requires [(valid server) (valid tool-name)]
  :ensures [(valid result)]
  :body
    (let (allowlist : _ (map-get-or server "allowlist" nil))
         (role : String (map-get server "role"))
      (if (= allowlist nil)
        true                          ;; no allowlist ⇒ allow everything
        (allowlist role tool-name))))
```

Signature changes from `(role, tool-name) → Bool` to
`(server, tool-name) → Bool`. Call sites inside AirTraffic update
accordingly. This also means `patch-tool-allowed.airl` style overrides
are no longer needed (or possible — the library function now reads
from the server record, not a global).

### B4. Remove `worker-id` from server state

The field is unused by AirTraffic itself. It's a workflow-identity leak.
Servers that care about worker identity can store it under any key they
want (e.g., in a separate "metadata" map).

---

## Consumer-side changes (mynameisAIRL)

### `src/tools.airl`

- Delete `airmunch-dispatch` (the if-chain). It becomes dead code.
- Replace `make-tool-defs` with `make-tool-bindings`, returning a list
  of `{schema, handler}` maps:

  ```airl
  (defn make-tool-bindings
    :sig [-> List]
    :body [
      (map-from ["schema" (map-from ["name" "airl_check_parens" ...])
                 "handler" handle-check-parens])
      (map-from ["schema" (map-from ["name" "airl_index_project" ...])
                 "handler" handle-index-project])
      ;; ...
    ])
  ```

- Handler signatures stay `(args : Map) -> Result`. The `Result` is
  serialized to MCP `tools/call` response by the library.
- The `cache` argument previously threaded through `airmunch-dispatch`
  needs a new home. Options:
  1. Wrap each handler at registration time in a closure capturing the
     cache: `(fn [args] (handle-index-project cache args))`.
  2. Promote cache to a top-level mutable reference (via `(atom)` or
     similar) that handlers consult directly.
  3. Redesign handlers to be cache-less (cache becomes a global
     singleton inside the index-store module).

  **Recommendation: (1).** Preserves the existing cache-as-parameter
  structure and localizes the capture to the binding list. Bindings
  become parameterized by `cache`.

### `mynameisairl.airl` (entry point)

Collapses from:

```airl
(let (server : Map (airtraffic-new "mynameisAIRL" (mni-version) "supervisor" ""))
  (let (server2 : Map (airtraffic-prompt server prompt-def))
    (let (server3 : Map (fold (fn [s t] (airtraffic-tool s t)) server2 (make-tool-defs)))
      (let (cache : Map (map-from []))
        (airtraffic-serve-batch server3 (fn [tool-name args]
          (airmunch-dispatch cache tool-name args)))))))
```

to:

```airl
(let (allowlist : _ (fn [role name] (starts-with? name "airl_")))
     (server : Map (airtraffic-new "mynameisAIRL" (mni-version) "" allowlist))
     (cache : Map (map-from []))
     (server2 : Map (airtraffic-prompt server prompt-def))
     (server3 : Map (airtraffic-tools server2 (make-tool-bindings cache)))
  (airtraffic-serve server3))
```

- No dispatch fn. No fold. No two-structure drift surface.
- No reliance on a hardcoded "supervisor" role — we pass an empty string
  and gate purely on the `airl_*` name prefix.
- No `patch-tool-allowed.airl`. Delete the file.

### Files to delete

- `servers/mynameisairl/patch-tool-allowed.airl` — obsoleted by the
  pluggable allowlist. Also remove its entry from `build.sh`.

## Test plan

1. **Fn-in-map AOT sanity** — already verified for named `defn` values
   (`handle-foo`/`handle-bar` test). Also verify the closure case —
   `(fn [args] (handle-foo cache args))` stored as a map value and
   invoked via `(map-get m name)` — since the mynameisAIRL cache-capture
   pattern depends on it. Add both cases as
   `tests/fixtures/valid/fn_in_map.airl` / `fn_in_map_closure.airl` so
   they become regression gates.
2. **AirTraffic unit-ish test** — in the AirTraffic repo: register a
   tool, call `airtraffic-handle-tools-list`, assert `_handler` is
   absent from the JSON output.
3. **mynameisAIRL end-to-end** — rebuild, issue MCP `tools/list` and
   `tools/call airl_check_parens` as in 2026-04-21. Expect all 12 tools
   advertised, `airl_check_parens` returns the paren report.
4. **Negative test** — call `tools/call` with an unregistered tool name
   and assert the library returns `{"error": "unknown tool: ..."}`
   rather than crashing.
5. **Leak test for `_handler`** — grep the `tools/list` response JSON;
   assert no occurrence of `_handler`.
6. **Allowlist enforcement** — create a server with allowlist
   `(fn [_role name] (= name "foo"))`, register tools "foo" and "bar",
   assert only "foo" appears in `tools/list` and `tools/call bar`
   returns `"unknown tool: bar"`.
7. **No-allowlist default** — create a server with `nil` allowlist,
   register any tool, assert it registers and dispatches without
   gating.
8. **`worker-id` absence** — assert `airtraffic-new` does not accept
   a `worker-id` argument and server state maps have no `worker-id`
   key.

## Migration path

Single coordinated landing across two repos. Both changes (A and B)
ship together because both break `airtraffic-new` / `airtraffic-tool`
signatures, and mynameisAIRL is the sole consumer for both:

1. Update `AirTraffic/src/airtraffic.airl`:
   - **A:** 3-arity `airtraffic-tool`, new `airtraffic-tools`, new
     `airtraffic-serve`, remove `airtraffic-serve-batch` (no shim),
     strip `_handler` in `airtraffic-handle-tools-list`, fix stale
     fn-in-map comment.
   - **B:** delete `airtraffic-role-tools` and `airtraffic-valid-role`.
     Change `airtraffic-new` to take an allowlist closure and remove
     `worker-id`. Change `airtraffic-tool-allowed` to read from the
     server's allowlist slot instead of a global function.
2. Update `AIRL/servers/mynameisairl/src/tools.airl`:
   - Replace `make-tool-defs` with `make-tool-bindings` (schema +
     handler pairs).
   - Delete `airmunch-dispatch`.
3. Update `AIRL/servers/mynameisairl/mynameisairl.airl`:
   - Pass a prefix-based allowlist closure to `airtraffic-new`.
   - Use `airtraffic-tools` + `airtraffic-serve`.
4. **Delete** `AIRL/servers/mynameisairl/patch-tool-allowed.airl` and
   remove it from `build.sh`'s file list.
5. Rebuild mynameisAIRL; run all test-plan items.
6. Add the fn-in-map fixtures from test plan item 1 to AIRL
   (regression gate).

**No third repo:** the workflow taxonomy is deleted rather than moved.
If the airl-workflow project ever implements an AIRL-side MCP server
that needs the supervisor/worker/critic role system, it'll supply its
own allowlist to `airtraffic-new`.

## Out of scope

- **MCP `prompts/list` drift.** Same pattern exists for prompts and
  resources but they have no dispatch — prompts and resources just
  serve static content. No drift risk.
- **Type-level enforcement of handler signature.** AIRL doesn't have
  a higher-kinded way to say "handler must be `(Map) -> Result`" as
  part of the tool-def type. Runtime assertion at dispatch is the best
  we can do today.
- **Middleware / hooks.** A future AirTraffic could add
  `airtraffic-wrap-tool` for cross-cutting logging/auth. Out of scope
  for this change.

## Risks

- **AOT path correctness under indirect call through map-get.** Spec 3
  tested named fns in maps directly. We should also test closures
  (e.g. `(fn [args] (handle-foo cache args))`) stored in maps and
  called dynamically, since the mynameisAIRL cache-capture pattern
  relies on that. Add to test plan item 1.
- **Map ordering.** Our `map-from` preserves insertion order; dispatch
  relies on `name` lookup, not order. No risk.
- **Other patch files in mynameisAIRL.** `patch-json-result.airl` and
  `patch-prompts-list.airl` don't touch tool-allowlist or registration
  paths — unaffected by either A or B. `patch-tool-allowed.airl` is
  deleted outright by step 4 of the migration.
- **The `role` field in `airtraffic-new` is now purely a label.**
  AirTraffic itself doesn't interpret it. If future consumers want
  typed role constants, they can layer validation on top. This is the
  intended shape for a generic framework.
- **Closure-based allowlists in the server state.** Server state is
  passed around by value (maps are persistent). Storing a closure in
  the map means each `airtraffic-tool` call duplicates a pointer to
  the same closure value, not the closure itself. No GC/lifetime
  surprises expected, but the fn-in-map closure sanity test (test plan
  item 1) covers this path too.

## Acceptance criteria

- mynameisAIRL entry point is a single `airtraffic-tools` call + a
  single `airtraffic-serve` call. No server-side dispatch if-chain.
- `tools/list` returns all 12 tools, none leaking `_handler`.
- `tools/call airl_check_parens` returns the expected paren report.
- Unknown tool name returns a clear error, not a crash.
- AirTraffic source no longer claims AOT rejects fn-in-map.
- AirTraffic source contains no references to `workflow_*` tool names,
  no hardcoded role taxonomy, and no `worker-id` field.
- mynameisAIRL no longer contains `patch-tool-allowed.airl` and still
  builds and serves correctly.
