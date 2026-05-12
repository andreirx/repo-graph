# FD-1B: Rust React Detector Parity

Status: IMPLEMENTED (2026-05-11)
Depends: None (uses existing ts-extractor TSX/JSX output + inferences substrate)
Follow-on: None

## Implementation Summary

AST-based React component and hook detection using tree-sitter-typescript. Integrated into compose.rs orchestration (compose-phase postpass after extraction).

### Artifacts

- `rust/crates/repo-index/src/react_detector.rs` — detection module (10 unit tests)
- `rust/crates/rgr/src/commands/inferences.rs` — inference query CLI (FD-SUPPORT-2)
- `rust/crates/storage/src/queries.rs` — `list_inferences_for_snapshot()` query
- `test/fixtures/typescript/react-frontend-corpus/` — validation corpus
- `rust/crates/repo-index/tests/fd_1b_react_integration.rs` — E2E integration test (5 tests)

### Validation Results (EXECUTED)

- 10 react_component inferences from corpus (exceeds 5-component acceptance)
- 14 react_hook_usage inferences from corpus (exceeds 5-hook acceptance)
- Negative cases correctly produce no inferences:
  - DataLoader (PascalCase but no JSX return): NOT detected
  - Utils.ts (non-React file): NOT detected
  - lowercase helper function: NOT detected

## Goal

Detect React component definitions and hook usage in TypeScript/JavaScript files. Emit frontend orientation hints as **inferences** (Layer 3), not nodes.

## Certainty Layer

**Layer 3 (Orientation Hints)**

React detection relies on heuristic structural matching. Provides orientation hints for frontend navigation, not strict architectural facts.

**Critical distinction:**
- `nodes` = Layer 0–1 extracted graph facts
- `inferences` = Layer 3 bounded hints

React component detection is heuristic, not deterministic. It belongs in `inferences`.

## Architecture

### Detection Layer
Compose-phase Rust React detector over TSX/JSX files (like FD-1A Express).

### Persistence Layer
`inferences` table with kinds:
- `react_component` — component definition evidence
- `react_hook_usage` — hook call evidence

### Read Layer
`rmap inferences list <db> <repo> --kind react_component`

**Note:** If inference query surface doesn't exist, include FD-SUPPORT-2 in this slice.

## Scope

### In Scope

**Component Detection:**
- PascalCase functions returning JSX → `react_component` inference
- Arrow functions with PascalCase name returning JSX → `react_component` inference
- `React.FC<T>` typed functions → `react_component` inference

**Hook Usage Detection (first cut):**
- Built-in hooks: `useState`, `useEffect`, `useContext`, `useReducer`, `useCallback`, `useMemo`, `useRef`
- Custom hooks: `use*` pattern with lowercase second letter

**Detection Gate:**
- File must import from `react` or `@types/react`
- File must have `.tsx` or `.jsx` extension (plain `.ts`/`.js` with JSX pragma NOT currently supported)

### Out of Scope

- Class components (`extends React.Component`)
- Component props analysis
- HOC (Higher-Order Component) detection
- Component hierarchy analysis
- HTTP surface emission (React components are NOT HTTP endpoints)

## Inference Schema

### Component Inference

```
kind: "react_component"
target_stable_key: {repo}:{file}#{component_name}:SYMBOL:FUNCTION
value_json: {
  "component_name": "UserProfile",
  "component_style": "function" | "arrow" | "fc_typed",
  "has_jsx_return": true,
  "import_gate": "react",
  "line_start": 15
}
confidence: 0.9 (PascalCase + JSX) | 0.7 (PascalCase only)
```

### Hook Usage Inference

```
kind: "react_hook_usage"
target_stable_key: {repo}:{file}#{caller_symbol}:SYMBOL:FUNCTION
value_json: {
  "hook_name": "useState",
  "hook_category": "builtin" | "custom",
  "caller_component": "UserProfile" | null,
  "line_start": 18
}
confidence: 0.9 (builtin) | 0.8 (custom)
```

## Crate Location

Same pattern as FD-1A Express:

```
rust/crates/repo-index/src/
├── compose.rs              # add persist_react_inferences call
├── express_detector.rs     # FD-1A (existing)
├── react_detector.rs       # FD-1B (NEW)
└── ...
```

## Validation Corpus

`test/fixtures/typescript/react-frontend-corpus/`

Required files:
- Functional components (PascalCase)
- Arrow function components
- `React.FC<Props>` typed components
- Built-in hook usage
- Custom hook definitions
- Negative: lowercase functions, non-React files

## Validation Commands

```bash
# 1. Build
cd rust && cargo build -p repo-graph-repo-index

# 2. Unit tests
cargo test -p repo-graph-repo-index react_detector

# 3. Index validation corpus
rmap index test/fixtures/typescript/react-frontend-corpus ./test-artifacts/fd-1b.db

# 4. List React component inferences
rmap inferences list ./test-artifacts/fd-1b.db react-frontend-corpus --kind react_component
# Expected: component count >= 5

# 5. List hook usage inferences
rmap inferences list ./test-artifacts/fd-1b.db react-frontend-corpus --kind react_hook_usage
# Expected: hook count >= 5

# 6. Verify NOT in surfaces
rmap surfaces list ./test-artifacts/fd-1b.db react-frontend-corpus --kind http_provider
# Expected: empty (React components are not HTTP surfaces)
```

## Acceptance Criteria

1. `react_detector.rs` module exists in `repo-index`
2. `detect_react_components()` extracts PascalCase functions with JSX
3. `detect_react_hooks()` extracts hook calls
4. Compose-phase wiring via `persist_react_inferences()`
5. Inferences persist to `inferences` table with correct kinds
6. `rmap inferences list --kind react_component` returns results
7. Validation corpus produces >= 5 components, >= 5 hooks
8. Negative cases produce no false positives
9. React components do NOT create `project_surfaces` rows

## Definition of Done

- Detection functional (criteria 1-5)
- Query surface working (criterion 6)
- Validation corpus exists and validates (criteria 7-8)
- Negative validation passes (criterion 9)
- E2E integration test exists

## Support Gap: Inference Query Surface

If `rmap inferences list` doesn't exist, this slice includes:

### FD-SUPPORT-2 (embedded)

Add `rmap inferences list <db> <repo> --kind <kind>` command.

Output format:
```json
{
  "command": "inferences list",
  "repo": "react-frontend-corpus",
  "kind": "react_component",
  "count": 12,
  "results": [
    {
      "inference_uid": "inf-abc123",
      "target_stable_key": "myrepo:src/UserProfile.tsx#UserProfile:SYMBOL:FUNCTION",
      "kind": "react_component",
      "value": { "component_name": "UserProfile", "component_style": "function" },
      "confidence": 0.9
    }
  ]
}
```

## Deferred

- Class components (`extends React.Component`)
- Component props extraction
- HOC detection
- TS prototype parity validation (first-cut Rust implementation)
