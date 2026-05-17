# CLI-OUT-1: Presentation Layer for Human-Default Output

**Status:** PLANNING  
**Priority:** HIGH (product surface quality)  
**Type:** CLI Architecture + UX  
**Depends on:** REG-1 (in progress)

## Problem Statement

Current CLI commands dump daemon response DTOs directly to stdout:

```rust
let result = client.request("orient", params)?;
println!("{}", serde_json::to_string_pretty(&result)?);
```

This creates three failures:

1. **Transport leakage** — internal envelope fields visible to users
2. **No human hierarchy** — all facts have equal visual weight
3. **No mode separation** — machine-readable JSON is the only output

The daemon DTO is the terminal presentation. That is a boundary violation.

## Target Contract

### Human mode (default)

```bash
$ rmap orient

Repo: billing-service
Status: oriented (partial confidence)

Signals
  - Boundary drift: payments -> notifications
  - 3 module ownership gaps
  - 1 stale policy declaration

Degradation
  - 2 files excluded (binary)
  - TypeScript coverage: 94%

Next steps
  - rmap modules violations
  - rmap boundaries summary
```

### Machine mode (explicit)

```bash
$ rmap orient --json
{"command":"orient","repo":"...","summary":{...},"signals":[...],...}
```

Same pattern for `check` and `explain`.

## Architectural Decisions

| ID | Decision | Choice |
|----|----------|--------|
| D1 | Default output mode | Human-readable plain text |
| D2 | Machine output | `--json` flag, returns full daemon envelope |
| D3 | Presentation location | `rgr/src/presentation/` module |
| D4 | Response typing | Typed structs for orient, check, explain |
| D5 | Terminal styling | None in this slice (plain text only) |
| D6 | Daemon contract | Unchanged — CLI renderer decides presentation |

## Scope

### In scope (phase 1)

| Command | Human Output | --json |
|---------|--------------|--------|
| `orient` | Status, signals, degradation, next steps | Full envelope |
| `check` | Pass/fail verdict, violation summary, actions | Full envelope |
| `explain` | Target summary, related facts, confidence | Full envelope |

### Out of scope

- ANSI color / terminal styling
- tty detection
- Rich tables / box drawing
- Pager integration
- Width-aware wrapping
- Commands beyond orient/check/explain
- Daemon protocol changes

## Implementation

### Presentation module structure

```
rust/crates/rgr/src/presentation/
  mod.rs           # shared helpers
  orient.rs        # orient renderer
  check.rs         # check renderer
  explain.rs       # explain renderer
```

### Shared helpers (presentation/mod.rs)

```rust
/// Render a section heading
pub fn heading(title: &str) -> String

/// Render a bulleted list
pub fn bullet_list(items: &[String]) -> String

/// Render a key-value line
pub fn kv_line(key: &str, value: &str) -> String

/// Render a degradation block (if any degradation present)
pub fn degradation_block(items: &[DegradationItem]) -> Option<String>

/// Render a "next steps" block with suggested commands
pub fn next_steps(commands: &[&str]) -> String
```

### Typed response structs

Each high-value command gets a typed response struct:

```rust
// presentation/orient.rs
#[derive(Deserialize)]
pub struct OrientResponse {
    pub repo: String,
    pub snapshot: String,
    pub summary: OrientSummary,
    pub signals: Vec<Signal>,
    pub degradation: Option<Degradation>,
    // ... fields needed for rendering
}

impl OrientResponse {
    pub fn render_human(&self) -> String {
        // ... plain text rendering
    }
}
```

### Command handler pattern

```rust
// commands/orient.rs
pub fn run_orient(args: &OrientArgs, client: &DaemonClient) -> Result<()> {
    let result = client.request("orient", params)?;
    
    if args.json {
        // Machine mode: print full envelope
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        // Human mode: parse and render
        let response: OrientResponse = serde_json::from_value(result)?;
        println!("{}", response.render_human());
    }
    Ok(())
}
```

## Human Output Design Principles

### 1. Summary first
Lead with:
- What object is being discussed
- Current status/verdict
- Decision-relevant counts only

### 2. Actionable degradation
If data is incomplete:
- State clearly what is degraded
- State what is missing
- Suggest next command if applicable

### 3. Hide internals
Do not show by default:
- `command` field
- `repo_uid`
- `snapshot_uid`
- Internal filter echoes
- Envelope scaffolding

### 4. Preserve certainty boundaries
Render distinctly:
- Extracted facts (high confidence)
- Inferred hints (bounded confidence)
- Policy/governance overlays

### 5. Suggest next steps
Orientation commands should end with actionable suggestions when relevant.

## Testing Strategy

### Human output tests

Structured assertion tests (not brittle snapshots):

```rust
#[test]
fn orient_human_shows_repo_name() {
    let response = OrientResponse { repo: "my-app".into(), ... };
    let output = response.render_human();
    assert!(output.contains("Repo: my-app"));
}

#[test]
fn orient_human_shows_signals_section() {
    let response = OrientResponse { signals: vec![...], ... };
    let output = response.render_human();
    assert!(output.contains("Signals"));
    assert!(output.contains("Boundary drift"));
}

#[test]
fn orient_human_hides_internal_fields() {
    let response = OrientResponse { ... };
    let output = response.render_human();
    assert!(!output.contains("snapshot_uid"));
    assert!(!output.contains("command\":"));
}
```

### JSON output tests

Existing daemon_dispatch.rs tests remain valid for `--json` mode.

Additional CLI-level tests:

```rust
#[test]
fn orient_json_flag_returns_full_envelope() {
    // Run with --json, verify envelope fields present
}
```

### Renderer unit tests

Test individual helpers:
- `heading()` formatting
- `bullet_list()` with empty/non-empty input
- `degradation_block()` with/without degradation
- `next_steps()` formatting

## Definition of Done

1. `orient`, `check`, `explain` have human-default plain text output
2. `--json` flag returns full structured envelope (backward compatible)
3. Internal envelope fields hidden from human output
4. Degradation rendered explicitly when present
5. `presentation/` module exists with shared helpers
6. Typed response structs for all three commands
7. Tests cover both human and JSON modes
8. No terminal styling / color logic

## Migration Path

1. Add `--json` flag to orient/check/explain (default: false)
2. Create presentation module with typed structs
3. Implement renderers
4. Switch default output to human mode
5. Verify existing --json tests still pass
6. Add human output tests

## Future Enhancements (not this slice)

- ANSI color with tty detection
- Rich tables for list commands
- Width-aware formatting
- Expand to modules/boundaries/surfaces commands
- Pager integration for long output

## Open Questions

None. Scope is locked to plain text, three commands, no styling.
