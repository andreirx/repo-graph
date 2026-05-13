# HOOK-1A: Stdin JSON Transport Adapter

Status: PLANNED
Depends: HOOK-1
Blocks: CLAUDE-1
Track: Distribution / Install / Host Integration

## Objective

Add stdin JSON transport to HOOK-1 hook commands. Claude Code passes hook context
as JSON on stdin, not environment variables. This slice adds the transport adapter
without changing policy handlers.

## Scope

Minimal extension to existing HOOK-1 implementation:

1. Add `--from-stdin` flag to all hook commands
2. Parse Claude Code JSON payload from stdin
3. Normalize into existing `HookContext` structure
4. Preserve `--from-env` for other hosts
5. Add validation tests

**Not in scope:** Policy handler changes, new hook events, enforcement mode.

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    rmap hook <cmd>                      │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐     │
│  │ --from-stdin│  │ --from-env  │  │ --db/--repo │     │
│  │ (NEW)       │  │ (existing)  │  │ (existing)  │     │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘     │
│         │                │                │             │
│         └────────────────┼────────────────┘             │
│                          ▼                              │
│                  ┌─────────────┐                        │
│                  │ HookContext │  (unchanged)           │
│                  └──────┬──────┘                        │
│                         ▼                               │
│                  ┌─────────────┐                        │
│                  │   Policy    │  (unchanged)           │
│                  │  Handlers   │                        │
│                  └─────────────┘                        │
└─────────────────────────────────────────────────────────┘
```

## Claude Code Stdin Payload

Claude Code passes JSON on stdin for each hook invocation:

### Common Fields

```json
{
  "session_id": "abc123-def456",
  "cwd": "/path/to/project",
  "hook_event_name": "SessionStart"
}
```

### Event-Specific Fields

**SessionStart:**
```json
{
  "session_id": "abc123",
  "cwd": "/path/to/project",
  "hook_event_name": "SessionStart"
}
```

**UserPromptSubmit:**
```json
{
  "session_id": "abc123",
  "cwd": "/path/to/project",
  "hook_event_name": "UserPromptSubmit",
  "prompt": "Implement the login feature"
}
```

**PostToolUse:**
```json
{
  "session_id": "abc123",
  "cwd": "/path/to/project",
  "hook_event_name": "PostToolUse",
  "tool_name": "Edit",
  "tool_input": {
    "file_path": "/path/to/file.rs",
    "old_string": "...",
    "new_string": "..."
  },
  "tool_output": "File edited successfully"
}
```

**PreCompact:**
```json
{
  "session_id": "abc123",
  "cwd": "/path/to/project",
  "hook_event_name": "PreCompact",
  "compaction_type": "auto"
}
```

**Stop:**
```json
{
  "session_id": "abc123",
  "cwd": "/path/to/project",
  "hook_event_name": "Stop"
}
```

## Implementation

### New Types

Add to `commands/hook/transport.rs` (new file):

```rust
use std::io::Read;
use std::path::PathBuf;
use serde::Deserialize;

/// JSON payload from Claude Code stdin transport.
#[derive(Debug, Deserialize)]
pub struct StdinPayload {
    pub session_id: Option<String>,
    pub cwd: PathBuf,
    pub hook_event_name: String,
    
    // Event-specific fields (all optional)
    pub prompt: Option<String>,
    pub tool_name: Option<String>,
    pub tool_input: Option<serde_json::Value>,
    pub tool_output: Option<String>,
    pub compaction_type: Option<String>,
}

impl StdinPayload {
    /// Read and parse JSON from stdin.
    pub fn from_stdin() -> Result<Self, String> {
        let mut input = String::new();
        std::io::stdin()
            .read_to_string(&mut input)
            .map_err(|e| format!("failed to read stdin: {}", e))?;
        
        if input.trim().is_empty() {
            return Err("no input on stdin".to_string());
        }
        
        serde_json::from_str(&input)
            .map_err(|e| format!("invalid JSON on stdin: {}", e))
    }
}
```

### HookContext Conversion

Add `From<StdinPayload>` implementation:

```rust
impl From<StdinPayload> for HookContext {
    fn from(payload: StdinPayload) -> Self {
        HookContext {
            host_type: HostType::ClaudeCode,
            repo_path: Some(payload.cwd),
            session_id: payload.session_id,
            tool_name: payload.tool_name,
            tool_output: payload.tool_output,
            prompt_text: payload.prompt,
        }
    }
}
```

### Flag Addition

Update each hook command's argument parsing to accept `--from-stdin`:

```rust
// In session_start.rs, prompt_submit.rs, post_edit.rs, etc.

let mut from_stdin = false;

// In argument parsing loop:
"--from-stdin" => {
    from_stdin = true;
    i += 1;
}

// In context resolution:
let ctx = if from_stdin {
    let payload = StdinPayload::from_stdin()?;
    HookContext::from(payload)
} else if from_env {
    HostContext::detect().into()
} else {
    // Explicit args or discovery
    HookContext::from_args(args)?
};
```

### Resolution Priority

Update resolution chain in `env.rs`:

```
1. Explicit --db/--repo arguments (highest priority)
2. RMAP_DB_PATH, RMAP_REPO_PATH environment variables
3. --from-stdin: parse JSON, use cwd as repo path
4. --from-env: read host environment variables
5. Discovery: find .rmap.db in current directory or parents
```

## Files to Modify

| File | Change |
|------|--------|
| `commands/hook/mod.rs` | Add `mod transport;` |
| `commands/hook/transport.rs` | New file: StdinPayload struct and parsing |
| `commands/hook/env.rs` | Add From<StdinPayload> for HookContext |
| `commands/hook/session_start.rs` | Add --from-stdin flag handling |
| `commands/hook/prompt_submit.rs` | Add --from-stdin flag handling |
| `commands/hook/post_edit.rs` | Add --from-stdin flag handling |
| `commands/hook/pre_compact.rs` | Add --from-stdin flag handling |
| `commands/hook/stop.rs` | Add --from-stdin flag handling |

## Testing

### Unit Tests

1. **Stdin parsing:**
   - Valid JSON parses correctly
   - Missing optional fields handled
   - Empty stdin returns error
   - Invalid JSON returns descriptive error

2. **Context normalization:**
   - StdinPayload converts to HookContext correctly
   - cwd maps to repo_path
   - session_id preserved
   - Event-specific fields mapped

3. **Flag precedence:**
   - Explicit args override stdin
   - --from-stdin and --from-env are mutually exclusive

### Integration Tests

1. **End-to-end stdin transport:**
   ```bash
   echo '{"session_id":"test","cwd":"/tmp","hook_event_name":"SessionStart"}' | \
     rmap hook session-start --from-stdin
   ```

2. **PostToolUse with tool data:**
   ```bash
   echo '{"cwd":"/tmp","hook_event_name":"PostToolUse","tool_name":"Edit","tool_output":"ok"}' | \
     rmap hook post-edit --from-stdin
   ```

## Validation Commands

```bash
# Unit tests
cargo test -p repo-graph-rgr stdin

# Manual validation
echo '{"session_id":"s1","cwd":"/tmp/test","hook_event_name":"SessionStart"}' | \
  ./target/debug/rmap hook session-start --from-stdin

# Error case: empty stdin
echo '' | ./target/debug/rmap hook session-start --from-stdin
# Expected: error about empty/missing stdin

# Error case: invalid JSON
echo 'not json' | ./target/debug/rmap hook session-start --from-stdin
# Expected: error about invalid JSON
```

## Deliverables

1. `StdinPayload` struct with serde deserialization
2. `--from-stdin` flag on all five hook commands
3. `From<StdinPayload> for HookContext` conversion
4. Unit tests for parsing and normalization
5. Integration tests for stdin transport
6. Updated help text mentioning --from-stdin

## Success Criteria

- All hook commands accept `--from-stdin` flag
- Claude Code JSON payloads parse correctly
- Existing `--from-env` behavior unchanged
- Policy handlers receive identical `HookContext` regardless of transport
- Tests pass for stdin transport path

## Out of Scope

- New hook events
- Policy handler changes
- Enforcement mode
- Output format changes
- Claude Code config patching (CLAUDE-1)
