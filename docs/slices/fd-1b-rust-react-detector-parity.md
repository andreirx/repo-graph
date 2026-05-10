# FD-1B: Rust React Detector Parity

Status: PLANNED
Depends: None (uses existing ts-extractor TSX/JSX output)
Follow-on: None

## Goal

Port the React component and hook detection logic from the legacy TypeScript codebase to Rust. Emit frontend orientation hints for UI navigation — NOT HTTP surfaces.

## Certainty Layer

**Layer 3 (Orientation Hints)**

React detection relies on heuristic structural matching. Provides orientation hints for frontend navigation, not strict architectural facts.

### Degradation Policy

When a capitalized function returns JSX but pattern is ambiguous:
- Emit with `confidence = 0.6`
- Tag as `possible_component`

When hook usage is detected but caller is not a component:
- Emit hook evidence anyway (custom hooks call hooks)
- Tag as `hook_usage` not `component`

## Scope

### In Scope

**Component Definition:**

A React component is:
- A function with PascalCase name returning JSX, OR
- A function assigned to `React.FC<T>` / `React.FunctionComponent<T>`, OR
- An arrow function with PascalCase name returning JSX

**Class components:** OUT for first cut (legacy pattern, low priority)

**Detector Output Contract:**

```rust
struct ReactComponentEvidence {
    kind: "react_component",
    component_name: String,
    component_style: "function" | "arrow" | "fc_typed",
    location: Location,
    confidence: f64,
}

struct ReactHookEvidence {
    kind: "react_hook_usage",
    hook_name: String,        // "useState", "useEffect", "useCustomHook"
    hook_category: "builtin" | "custom",
    caller_component: Option<String>,
    location: Location,
}
```

**Built-in Hooks (first cut):**
- `useState`
- `useEffect`
- `useContext`
- `useReducer`
- `useCallback`
- `useMemo`
- `useRef`

**Custom Hooks:**
- Any function starting with `use` and lowercase second letter
- e.g., `useAuth`, `useFetch`, `useLocalStorage`
- Emitted as `hook_category: "custom"`

**NOT HTTP Surfaces:**
- This detector emits `ReactComponentEvidence` and `ReactHookEvidence`
- These are frontend orientation hints
- They do NOT create `project_surface` records
- They do NOT appear in `rmap surfaces list`

### Out of Scope

- Class components (`extends React.Component`)
- HTTP surface emission
- `fetch`/`axios` call detection (state-boundary slices)
- Component props analysis
- HOC (Higher-Order Component) detection

## Crate Layout

```
rust/crates/detectors/
├── src/
│   ├── lib.rs                    # Detector registry
│   ├── express/                  # FD-1A
│   ├── react/
│   │   ├── mod.rs                # React detector entry
│   │   ├── components.rs         # Component detection
│   │   ├── hooks.rs              # Hook usage detection
│   │   └── evidence.rs           # Evidence structs
│   └── traits.rs                 # FrameworkDetector trait
└── tests/
    ├── react_components.rs
    ├── react_hooks.rs
    └── fixtures/
        ├── simple_component.tsx
        ├── hooks_usage.tsx
        └── custom_hooks.ts

rust/crates/indexer/
└── src/
    └── framework_hints.rs        # Stores evidence in nodes table
```

## Prerequisites

- `ts-extractor` emits:
  - `FUNCTION` nodes with name, return type
  - `JSX_ELEMENT` detection in return position
  - `CALL` nodes for hook invocations
- Node `kind` extensible for `REACT_COMPONENT` / `REACT_HOOK`

## Validation Corpus

Repository: `test/fixtures/typescript/react-frontend-corpus/`

Must contain:
- Functional components (PascalCase)
- Arrow function components
- `React.FC<Props>` typed components
- Built-in hook usage (`useState`, `useEffect`)
- Custom hook definitions
- At least 10 components, 5 custom hooks

## Validation Commands

**Note:** No `rmap` query surface exists yet for React component/hook evidence. This is a temporary validation limitation. Primary validation uses `rmap callers` to find component symbols; secondary validation uses storage diagnostics.

```bash
# 1. Build
cd rust && cargo build -p repo-graph-detectors

# 2. Unit tests
cargo test -p repo-graph-detectors react

# 3. Index validation corpus (product surface)
rmap index test/fixtures/typescript/react-frontend-corpus ./test-artifacts/fd-1b.db

# 4. Primary validation: query component symbols via callers
rmap callers ./test-artifacts/fd-1b.db react-frontend-corpus "UserProfile"
# Must find the component if it exists in corpus

# 5. Verify components NOT exposed as HTTP surfaces
rmap surfaces list ./test-artifacts/fd-1b.db react-frontend-corpus --kind http_provider
# Must return empty or no React-related surfaces

# 6. Secondary diagnostic: storage query for evidence (interim)
# This is temporary until rmap exposes framework hints
sqlite3 ./test-artifacts/fd-1b.db \
  "SELECT name, kind FROM nodes WHERE kind = 'REACT_COMPONENT' LIMIT 5"
# Expected: component names visible

# 7. Hook evidence diagnostic
sqlite3 ./test-artifacts/fd-1b.db \
  "SELECT COUNT(*) FROM nodes WHERE kind = 'REACT_COMPONENT'"
# Expected: ≥10
```

## Acceptance Criteria

**Detection (detector responsibility):**
1. `ReactDetector` implements `FrameworkDetector` trait
2. Functional components: PascalCase + JSX return → `REACT_COMPONENT` node kind
3. `React.FC<T>` typed functions → `REACT_COMPONENT` node kind
4. Built-in hooks: 7 hooks detected when used
5. Custom hooks: `use*` pattern → `hook_category: "custom"`

**Semantic Examples:**
6. `function UserProfile() { return <div>...</div> }` → node with `kind: REACT_COMPONENT`, `name: UserProfile`
7. `const App: React.FC = () => <Main />` → node with `kind: REACT_COMPONENT`, `name: App`
8. `const [count, setCount] = useState(0)` → hook evidence with `hook_name: useState`

**Negative Examples:**
9. `function fetchData() { ... }` (lowercase, no JSX) → NOT a component
10. React components do NOT create `http_provider` surfaces

**Validation Limitation:**
11. Until `rmap` exposes framework hints, secondary validation uses storage queries
12. **Accept that React detection remains partially validated until a proper query surface exists**
13. Component/hook count within ±15% of legacy TS detector
14. `cargo test -p repo-graph-detectors` — all pass

## Definition of Parity

"Parity" for this slice means:
- **Component count:** Within ±15% of legacy TS detector
- **Hook detection:** Built-in hooks recognized
- **Custom hook detection:** `use*` pattern recognized

NOT required:
- Exact confidence values
- Class component detection
- Props type extraction
- Component hierarchy analysis

## Alternatives Considered

### A. Include class components
Rejected: Class components are legacy pattern. Function components are >90% of modern React. Add later if needed.

### B. Create HTTP surfaces for data-fetching components
Rejected: Conflates frontend structure with network boundaries. Data fetching is state-boundary concern.

### C. Analyze component props
Deferred: Props are useful but adds significant complexity. Component existence is sufficient for orientation.
