# AirTraffic Unified Tool Registration

**Date:** 2026-04-22
**Status:** Design approved, ready for implementation plan
**Scope:** AirTraffic library (`~/repos/AirTraffic`) + mynameisAIRL (only
current consumer)

## Problem

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

## Why the obvious fix didn't happen sooner

`airtraffic.airl:83-84` claims g3 AOT does not support functions as map
values, which would rule out bundling the handler into the tool-def map.
Verified 2026-04-22: the comment is stale. Post-Spec-3 (BCFuncNative),
`(map-get handlers name)` returning a function and being called via
`(h args)` works in AOT. Test case:

```airl
(defn handle-foo :sig [(args : Map) -> String] :body "foo")
(defn handle-bar :sig [(args : Map) -> String] :body "bar")

(let (handlers : Map (map-from ["foo" handle-foo "bar" handle-bar]))
     (h : _ (map-get handlers "bar"))
     (result : String (h (map-from [])))
  (eprintln result))
;; → prints "bar"
```

Compiled through the AOT path, runs correctly. The stale comment blocked
the natural design and forced the two-structure (schema-list +
dispatcher-fn) pattern that allows drift.

## Design

Collapse schema and handler into one map entry. Dispatch derives from
registered tools.

### New `airtraffic-tool` (3-arity)

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

### New `airtraffic-tools` (list variant — ergonomic)

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

### New `airtraffic-serve` (replaces `airtraffic-serve-batch`)

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

### `airtraffic-handle-tools-list` must strip `_handler`

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

### Deprecation of `airtraffic-serve-batch`

Keep it as a thin shim that registers a no-op tool set and uses the old
batch dispatcher — OR remove it entirely. mynameisAIRL is the only known
consumer, and it's moving to the new API in the same PR. **Decision:
remove it.** Cleanliness > backward-compat for a library with one
consumer.

### Remove stale fn-in-map comment

`airtraffic.airl:83-84` currently reads:

> ;; Note: "content" is a static string. g3 AOT does not support fn-in-map,
> ;; so content-fn is not supported — provide the content string directly.

Replace with a factual note about the current design (static content
only for resources, because per-request generation requires passing
request context, which isn't wired through `airtraffic-handle-resources-read`
today). Do not retain the false claim about AOT.

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
(let (server : Map (airtraffic-new "mynameisAIRL" (mni-version) "supervisor" ""))
     (cache : Map (map-from []))
     (server2 : Map (airtraffic-prompt server prompt-def))
     (server3 : Map (airtraffic-tools server2 (make-tool-bindings cache)))
  (airtraffic-serve server3))
```

No dispatch fn. No fold. No two-structure drift surface.

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

## Migration path

Single PR spanning both repos (AirTraffic and AIRL/servers/mynameisairl),
since the breaking change requires coordinated updates:

1. Update `AirTraffic/src/airtraffic.airl`:
   - 3-arity `airtraffic-tool`
   - new `airtraffic-tools`
   - new `airtraffic-serve`
   - remove `airtraffic-serve-batch` (no shim — mynameisAIRL is the
     only consumer and moves in the same PR)
   - strip `_handler` in `airtraffic-handle-tools-list`
   - fix stale comment
2. Update `AIRL/servers/mynameisairl/src/tools.airl`:
   - replace `make-tool-defs` with `make-tool-bindings`
   - delete `airmunch-dispatch`
3. Update `AIRL/servers/mynameisairl/mynameisairl.airl`:
   - use `airtraffic-tools` + `airtraffic-serve`
4. Rebuild mynameisAIRL, run test plan items 3–5.
5. Optional: add the fn-in-map fixture from test plan item 1 to AIRL.

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
- **Patch files in mynameisAIRL.** `patch-tool-allowed.airl` overrides
  `airtraffic-tool-allowed`. That override fires inside the new
  `airtraffic-tool` before `_handler` is attached — still works.
  `patch-json-result.airl` and `patch-prompts-list.airl` don't touch
  tool paths — unaffected.

## Acceptance criteria

- mynameisAIRL entry point is a single `airtraffic-tools` call + a
  single `airtraffic-serve` call. No server-side dispatch if-chain.
- `tools/list` returns all 12 tools, none leaking `_handler`.
- `tools/call airl_check_parens` returns the expected paren report.
- Unknown tool name returns a clear error, not a crash.
- AirTraffic source no longer claims AOT rejects fn-in-map.
