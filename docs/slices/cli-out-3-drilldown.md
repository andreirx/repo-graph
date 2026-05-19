# CLI-OUT-3: Graph Drilldown Output

**Status:** COMPLETE  
**Type:** Product Surface / Implementation  
**Prerequisite:** CLI-OUT-2B, CLI-OUT-2C

## Problem Statement

`rmap callers`, `rmap callees`, `rmap path`, and `rmap imports` currently dump raw JSON.
Users need scannable human output for graph drilldown queries.

`rmap explain` already has a human renderer (implemented previously).

## Scope Constraint

**Primarily renderer work.** One daemon-side change required:

Ambiguous symbol errors currently return prose in error message. Need structured
payload for clean CLI rendering. See "Ambiguous Symbol Handling" section below.

## Commands in Scope

| Command | Current State | Needed |
|---------|--------------|--------|
| `rmap callers <symbol>` | Raw JSON | Human renderer |
| `rmap callees <symbol>` | Raw JSON | Human renderer |
| `rmap path <from> <to>` | Raw JSON | Human renderer |
| `rmap imports <file>` | Raw JSON | Human renderer |
| `rmap explain <target>` | Human renderer exists | None |

## Data Available (from daemon responses)

### callers / callees

```json
{
  "target": {
    "stable_key": "...",
    "name": "State",
    "qualified_name": "OpenXcom::State::State",
    "kind": "SYMBOL",
    "subtype": "CONSTRUCTOR",
    "file": "src/Engine/State.cpp",
    "line": 51,
    "column": 0
  },
  "callers": [  // or "callees"
    {
      "stable_key": "...",
      "name": "getCursor",
      "qualified_name": "OpenXcom::Game::getCursor",
      "kind": "SYMBOL",
      "subtype": "METHOD",
      "file": "src/Engine/Game.cpp",
      "line": 417,
      "column": 0,
      "edge_type": "CALLS",
      "resolution": "static"
    }
  ],
  "count": 1
}
```

### path

```json
{
  "repo_uid": "...",
  "snapshot_uid": "...",
  "path": {
    "found": true,
    "path_length": 3,
    "path": [
      { "name": "A", "file": "a.cpp", "line": 10 },
      { "name": "B", "file": "b.cpp", "line": 20 },
      { "name": "C", "file": "c.cpp", "line": 30 }
    ]
  },
  "found": true
}
```

### imports

```json
{
  "file": "src/Engine/State.cpp",
  "imports": [
    {
      "node_id": "...",
      "symbol": "src/Interface/BattlescapeButton.h",
      "kind": "FILE",
      "subtype": "SOURCE",
      "file": "src/Interface/BattlescapeButton.h",
      "line": 1,
      "column": 0,
      "edge_type": "IMPORTS",
      "resolution": "static",
      "evidence": ["cpp-core:0.1.0"],
      "depth": 1
    }
  ]
}
```

## Proposed Human Output Formats

### callers

```
Callers of OpenXcom::State::State
File: src/Engine/State.cpp:51

5 callers found

  OpenXcom::Game::run          src/Engine/Game.cpp:234     CALLS  static
  OpenXcom::Menu::init         src/Menu/Menu.cpp:45        CALLS  static
  OpenXcom::Battle::setup      src/Battle/Battle.cpp:89    CALLS  static
  OpenXcom::Geo::enter         src/Geo/Geo.cpp:112         CALLS  static
  OpenXcom::Base::open         src/Base/Base.cpp:67        CALLS  static
```

### callees

```
Callees of OpenXcom::State::State
File: src/Engine/State.cpp:51

3 callees found

  OpenXcom::Game::getCursor    src/Engine/Game.cpp:417     CALLS  static
  OpenXcom::Screen::init       src/Engine/Screen.cpp:23    CALLS  static
  OpenXcom::Palette::load      src/Engine/Palette.cpp:89   CALLS  static
```

### path

When found:
```
Path: OpenXcom::State::State -> OpenXcom::Game::run

3 hops

  OpenXcom::State::State       src/Engine/State.cpp:51
    -> CALLS
  OpenXcom::Game::getCursor    src/Engine/Game.cpp:417
    -> CALLS
  OpenXcom::Game::run          src/Engine/Game.cpp:234
```

When not found:
```
Path: OpenXcom::State::State -> OpenXcom::Game::run

No path found.
```

### imports

```
Imports: src/Engine/State.cpp

19 imports

  src/Engine/Game.h            depth=1  static
  src/Engine/InteractiveSurface.h  depth=1  static
  src/Engine/Language.h        depth=1  static
  src/Engine/LocalizedText.h   depth=1  static
  src/Engine/Palette.h         depth=1  static
  src/Engine/Screen.h          depth=1  static
  src/Engine/State.h           depth=1  static
  src/Engine/Surface.h         depth=1  static
  src/Interface/BattlescapeButton.h  depth=1  static
  src/Interface/ComboBox.h     depth=1  static
  ...
```

## Design Rationale

1. **Target identification first** - show what was queried
2. **Count before list** - reader knows scale immediately
3. **Tabular alignment** - scannable without parsing
4. **Edge type and resolution visible** - trust signal per edge
5. **Full paths preserved** - no truncation
6. **`--json` for machine mode** - consistent with other commands

## Definition of Done

### Renderer Implementation
- [x] `presentation/graph_edges.rs` with shared callers/callees human renderer
- [x] `presentation/path.rs` with human renderer (query-term-preserving header)
- [x] `presentation/imports.rs` with human renderer
- [x] `run_callers`, `run_callees`, `run_path`, `run_imports` updated to support `--json` flag
- [x] Human output is default, `--json` returns full envelope
- [x] Structured ambiguous symbol errors with daemon-side data payload
- [x] CLI renders ambiguous symbol list with hint

### Renderer Unit Tests
- [x] graph_edges renderer tests (6 tests)
- [x] path renderer tests (6 tests, including query-term-preserving not-found)
- [x] imports renderer tests (7 tests)

### Daemon Dispatch Success-Path Tests (in daemon_dispatch.rs)
- [x] `callers_returns_success_response_structure`
- [x] `callees_returns_success_response_structure`
- [x] `path_returns_success_response_structure`
- [x] `path_not_found_returns_proper_structure`
- [x] `imports_returns_success_response_structure`
- [x] `callers_ambiguous_symbol_returns_structured_error`

### CLI Integration Tests (in cli_out_3_drilldown.rs)
- [x] `callers_human_mode_contains_structured_markers`
- [x] `callers_json_mode_returns_valid_envelope`
- [x] `callees_human_mode_contains_structured_markers`
- [x] `callees_json_mode_returns_valid_envelope`
- [x] `path_human_mode_shows_route`
- [x] `path_not_found_preserves_query_terms_in_human_output`
- [x] `path_json_mode_returns_valid_envelope`
- [x] `imports_human_mode_shows_file_imports`
- [x] `imports_json_mode_returns_valid_envelope`
- [x] `ambiguous_symbol_renders_numbered_list_with_hint`

### Validation Artifact
- [x] Review packet at `docs/audits/cli-out-3/review-packet.md`
- [x] Corpus evidence for OpenXcom, django, duckdb

### Known Test Debt

**TD-CLI-OUT-3-A: CLI integration tests are opt-in**

The `cli_out_3_drilldown.rs` tests are marked `#[ignore]` and require manual
`rmapd` pre-build. This matches the existing pattern in `cli_output_mode.rs`
(TD-CLI-OUT-1-A). The tests exist as proof surface but are not part of the
default `cargo test` path.

Run explicitly with:
```
cargo build -p rmapd
cargo test -p repo-graph-rgr --test cli_out_3_drilldown -- --ignored
```

## Files in Scope

### Daemon (transport layer)
- `rust/crates/daemon-transport/src/envelope.rs` (add AmbiguousSymbol code, data field)

### Daemon (dispatch)
- `rust/crates/daemon-runtime/src/dispatch.rs` (structured ambiguity response)

### CLI (presentation)
- `rust/crates/rgr/src/presentation/graph_edges.rs` (create - shared callers/callees support)
- `rust/crates/rgr/src/presentation/path.rs` (create)
- `rust/crates/rgr/src/presentation/imports.rs` (create)
- `rust/crates/rgr/src/presentation/mod.rs` (add modules)

### CLI (commands)
- `rust/crates/rgr/src/commands/graph.rs` (update run_callers, run_callees, run_path, run_imports)
- `rust/crates/rgr/src/daemon_client/connection.rs` (handle AmbiguousSymbol error)

## Explicit Non-Goals

- Do not change explain (already has human renderer)
- Do not add new query capabilities
- Do not change daemon response structure
- Do not add colors/styling (future slice)

## Design Decisions (Resolved)

1. **Shared renderer for callers/callees**: YES. Use `presentation/graph_edges.rs` support
   module with thin command-specific wrappers. These commands change for the same reasons
   (CCP), so one place to evolve ordering, headings, evidence labels.

2. **Large result sets**: Output all by default. No default limit. User can pipe to `head`.
   Consistent with stats approach. No silent evidence withholding.

3. **Ambiguous symbol handling**: Structured rendering required. See section below.

## Ambiguous Symbol Handling

**Current state**: Daemon returns prose error:
```
error: InvalidRequest: ambiguous symbol 'State', matches: repo_...:src/Engine/State.cpp#..., repo_...:src/lodepng.cpp#...
```

**Required state**: Structured error with renderable data.

### Daemon Change

1. Add `AmbiguousSymbol` error code to `ErrorCode` enum
2. Add optional `data: Option<Value>` field to `ErrorDetail`
3. When ambiguity detected, return:
   ```json
   {
     "id": "...",
     "error": {
       "code": "AmbiguousSymbol",
       "message": "symbol 'State' is ambiguous",
       "data": {
         "query": "State",
         "matches": [
           {
             "qualified_name": "OpenXcom::State::State",
             "kind": "CONSTRUCTOR",
             "file": "src/Engine/State.cpp",
             "line": 51
           },
           {
             "qualified_name": "lodepng::State::State",
             "kind": "CONSTRUCTOR",
             "file": "src/lodepng.cpp",
             "line": 234
           }
         ]
       }
     }
   }
   ```

### CLI Rendering

```
error: symbol 'State' is ambiguous

Matches:
  1. OpenXcom::State::State    CONSTRUCTOR  src/Engine/State.cpp:51
  2. lodepng::State::State     CONSTRUCTOR  src/lodepng.cpp:234

hint: use qualified name for exact match
```

This format:
- Shows error header
- Lists candidates with identity (qualified name, kind, location)
- Points user to next action
