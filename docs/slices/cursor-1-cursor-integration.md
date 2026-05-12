# CURSOR-1: Cursor MCP/Rules Integration

Status: PLANNED
Depends: HOST-1
Track: Distribution / Install / Host Integration

**Note:** This slice does NOT depend on HOOK-1. Cursor uses MCP (tool exposure),
not lifecycle hooks. The `rmap hook` commands are irrelevant here. Instead, this
slice may require a separate MCP support surface (`rmap mcp serve`).

## Objective

Implement repo-graph integration with Cursor via MCP (Model Context Protocol) server
and project rules. Cursor does not use lifecycle hooks like Claude Code/Codex —
integration is through tool exposure and context rules.

## Assumptions to Verify

**The following are based on Cursor documentation. Verify against actual Cursor
behavior before implementation.**

- MCP config file location (`~/.cursor/mcp.json` assumed)
- MCP server stdio protocol compatibility
- Rules file location and format
- Tool invocation patterns

Sources:
- https://docs.cursor.com/cli/mcp
- https://docs.cursor.com/en/context/rules

## Cursor Integration Model

Cursor provides two integration mechanisms:

1. **MCP (Model Context Protocol):** External tools exposed to the agent
2. **Rules:** Project-level instructions that guide agent behavior

These are different from lifecycle hooks:
- No automatic execution at session start/stop
- Agent must choose to use exposed tools
- Rules provide guidance, not enforcement

## MCP Server

### Server Specification

repo-graph exposes an MCP server that Cursor can connect to:

```
Command: rmap mcp serve
Protocol: stdio (stdin/stdout JSON-RPC)
```

### Tools Exposed

| Tool | Description | Parameters |
|------|-------------|------------|
| `rmap_orient` | Get orientation summary | `repo_path` |
| `rmap_trust` | Get trust snapshot | `repo_path` |
| `rmap_callers` | Find callers of symbol | `repo_path`, `symbol` |
| `rmap_callees` | Find callees of symbol | `repo_path`, `symbol` |
| `rmap_boundaries` | List boundary surfaces | `repo_path`, `kind?` |
| `rmap_modules` | List modules | `repo_path` |
| `rmap_check` | Run validation | `repo_path` |
| `rmap_refresh` | Refresh index | `repo_path` |

### Tool Schema (JSON-RPC)

```json
{
  "name": "rmap_orient",
  "description": "Get repo-graph orientation summary for the repository",
  "inputSchema": {
    "type": "object",
    "properties": {
      "repo_path": {
        "type": "string",
        "description": "Path to repository root"
      }
    },
    "required": ["repo_path"]
  }
}
```

### MCP Server Implementation

```rust
// Conceptual structure
pub struct RmapMcpServer {
    daemon_client: Option<DaemonClient>,
}

impl RmapMcpServer {
    pub async fn handle_tool_call(&self, name: &str, args: Value) -> Result<Value> {
        match name {
            "rmap_orient" => self.orient(args).await,
            "rmap_trust" => self.trust(args).await,
            "rmap_callers" => self.callers(args).await,
            // ...
        }
    }
}
```

## Cursor Configuration

### MCP Configuration

`~/.cursor/mcp.json` (or platform equivalent):

```json
{
  "mcpServers": {
    "repo-graph": {
      "command": "rmap",
      "args": ["mcp", "serve"],
      "env": {}
    }
  }
}
```

### Project Rules

`.cursor/rules` or `.cursorrules`:

```markdown
## repo-graph Integration

This project uses repo-graph for code intelligence. The following tools are available:

### Before Making Changes

1. Use `rmap_orient` to understand the current repository structure
2. Use `rmap_trust` to check for known gaps or issues
3. Use `rmap_modules` to understand module boundaries

### When Investigating Code

- Use `rmap_callers` to find what calls a function
- Use `rmap_callees` to find what a function calls
- Use `rmap_boundaries` to find API surfaces and IPC points

### After Making Changes

1. Use `rmap_refresh` to update the index
2. Use `rmap_check` to validate the changes

### Important

- Always check trust status before relying on repo-graph data
- If index is stale, run refresh before making decisions
- Boundary and module information helps understand system structure
```

## Integration Commands

### rmap integrate cursor

```
$ rmap integrate cursor [--global|--project]

Options:
  --global    Install MCP server globally (~/.cursor/mcp.json)
  --project   Install rules in current project (.cursor/rules)
  --both      Install both MCP and project rules (default)
  --dry-run   Show changes without applying
```

**Behavior:**

1. Detect Cursor installation
2. Check for existing MCP config
3. Add repo-graph MCP server entry
4. Create/update project rules
5. Verify MCP server works

### rmap integrate --status cursor

```
$ rmap integrate --status cursor

Cursor Integration Status

MCP Server:
  Config: ~/.cursor/mcp.json
  Status: installed
  Server: rmap mcp serve
  Tools: 8 tools exposed

Project Rules:
  Config: ./.cursor/rules
  Status: installed
  
MCP Health:
  ✓ Server starts correctly
  ✓ Tools respond to queries
```

### rmap mcp serve

```
$ rmap mcp serve [--debug]

Options:
  --debug    Enable debug logging to stderr
```

Starts MCP server on stdio. Cursor launches this automatically.

## MCP Server Protocol

### Initialization

```json
// Request
{"jsonrpc": "2.0", "method": "initialize", "id": 1}

// Response
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "protocolVersion": "2024-11-05",
    "serverInfo": {
      "name": "repo-graph",
      "version": "0.1.0"
    },
    "capabilities": {
      "tools": {}
    }
  }
}
```

### List Tools

```json
// Request
{"jsonrpc": "2.0", "method": "tools/list", "id": 2}

// Response
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "tools": [
      {
        "name": "rmap_orient",
        "description": "Get repo-graph orientation summary",
        "inputSchema": {...}
      },
      // ...
    ]
  }
}
```

### Call Tool

```json
// Request
{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "id": 3,
  "params": {
    "name": "rmap_orient",
    "arguments": {
      "repo_path": "/path/to/repo"
    }
  }
}

// Response
{
  "jsonrpc": "2.0",
  "id": 3,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "Repository: my-repo\nModules: 12\nBoundaries: 8\n..."
      }
    ]
  }
}
```

## Differences from Hook Integration

| Aspect | Hooks (Claude/Codex) | MCP (Cursor) |
|--------|---------------------|--------------|
| Execution | Automatic at events | Agent-initiated |
| Enforcement | Can block workflow | Advisory only |
| Session state | Managed by hooks | Agent must manage |
| Refresh | Automatic post-edit | Agent must call |

### Implications

1. **No automatic orientation:** Agent must call `rmap_orient` explicitly
2. **No automatic refresh:** Agent must call `rmap_refresh` after edits
3. **Rules are advisory:** Agent may ignore rules
4. **No enforcement possible:** Cannot block completion

## Project Rules Best Practices

### Keep Rules Concise

```markdown
## repo-graph

Tools: rmap_orient, rmap_trust, rmap_callers, rmap_callees, rmap_boundaries, rmap_modules, rmap_check, rmap_refresh

Workflow:
1. `rmap_orient` before changes
2. `rmap_refresh` after changes
3. `rmap_check` before completion
```

### Project-Specific Rules

Projects can customize rules:

```markdown
## repo-graph (project-specific)

This is a microservices project. Pay attention to:
- `rmap_boundaries` for service boundaries
- Module dependencies across services
- gRPC contract links

When modifying API:
1. Check `rmap_boundaries` for consumers
2. Verify contract links are not broken
```

## Testing

### MCP Server Tests

- Server starts and responds to initialize
- All tools listed correctly
- Each tool responds with valid output
- Error handling works

### Integration Tests

- MCP config installs correctly
- Cursor can connect to server
- Tools work from Cursor
- Rules are readable

## Error Handling

### Server Startup Failure

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "error": {
    "code": -32603,
    "message": "Failed to start: daemon not running"
  }
}
```

### Tool Execution Error

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "error": {
    "code": -32000,
    "message": "Repository not indexed",
    "data": {
      "suggestion": "Run: rmap index /path/to/repo ./repo.db"
    }
  }
}
```

## Out of Scope (CURSOR-1)

- Resources (MCP resources feature)
- Prompts (MCP prompts feature)
- Cursor-specific lifecycle events (if added later)
- Enforcement mechanisms

## Deliverables

1. `rmap mcp serve` command
2. MCP server implementation (JSON-RPC over stdio)
3. Tool implementations for all 8 tools
4. `rmap integrate cursor` command
5. Project rules template
6. MCP protocol tests
7. Integration documentation

## Success Criteria

- MCP server starts and handles requests
- All tools work correctly
- Cursor can connect and use tools
- Project rules install correctly
- Documentation is clear about differences from hook model
