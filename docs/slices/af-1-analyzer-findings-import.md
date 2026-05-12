# AF-1: Analyzer Findings Import

Status: PLANNED
Depends: TC-1 (toolchain inventory), BC-1 (build context)
Unblocks: Risk overlay enrichment, security surface discovery
Track: Toolchain-Aware Evidence Import
Layer: 3 (orientation hints — imported findings)

## Goal

Import findings from LLVM sanitizers (ASan, UBSan, TSan, MSan) and static
analyzers (clang static analyzer, clang-tidy) as risk/quality evidence.

**Rationale:** Sanitizer and analyzer findings are high-signal evidence for:
- Memory safety issues (ASan)
- Undefined behavior (UBSan)
- Data races (TSan)
- Code quality issues (clang-tidy)
- Security vulnerabilities (static analyzer)

These are not structural facts but imported evidence that enriches risk scoring
and hotspot interpretation. An agent should know which functions have known
issues flagged by authoritative tools.

## Problem Analysis

### Current State

- No sanitizer finding import
- No static analyzer finding import
- Risk scoring based only on churn × complexity
- No security/quality signal from external tools

### Finding Sources

| Tool | Output Format | Finding Types |
|------|---------------|---------------|
| ASan | stderr, ASAN_OPTIONS=log_path | heap-buffer-overflow, use-after-free, etc. |
| UBSan | stderr, UBSAN_OPTIONS=log_path | signed-integer-overflow, null-pointer, etc. |
| TSan | stderr, TSAN_OPTIONS=log_path | data-race, thread-leak, etc. |
| clang-tidy | stdout, YAML fixes | modernize-*, bugprone-*, cert-*, etc. |
| scan-build | plist, HTML | logic errors, memory leaks, etc. |

### Report Formats

**Sanitizer stderr format:**
```
==12345==ERROR: AddressSanitizer: heap-buffer-overflow on address 0x...
    #0 0x... in function_name /path/to/file.c:42:10
    #1 0x... in caller_name /path/to/file.c:100:5
```

**clang-tidy output:**
```
/path/to/file.c:42:10: warning: use of 'strcpy' is dangerous [bugprone-not-null-terminated-result]
```

**clang-tidy YAML fixes:**
```yaml
---
MainSourceFile: /path/to/file.c
Diagnostics:
  - DiagnosticName: bugprone-not-null-terminated-result
    DiagnosticMessage:
      Message: "use of 'strcpy' is dangerous"
      FileOffset: 1234
      FilePath: /path/to/file.c
    Replacements: [...]
```

**scan-build plist:**
```xml
<dict>
  <key>files</key>
  <array><string>/path/to/file.c</string></array>
  <key>diagnostics</key>
  <array>
    <dict>
      <key>category</key><string>Memory error</string>
      <key>type</key><string>Memory leak</string>
      <key>location</key>
      <dict>
        <key>line</key><integer>42</integer>
        <key>file</key><integer>0</integer>
      </dict>
    </dict>
  </array>
</dict>
```

## Scope

### Phase 1: clang-tidy (Lowest Friction)

clang-tidy is the easiest entry point:
- Structured output available (YAML, JSON in newer versions)
- Runs on individual files without full build
- Categorized findings (bugprone, cert, modernize, etc.)
- Already present via Xcode toolchain

**In Scope Phase 1:**
- clang-tidy YAML/text output parser
- Finding model: file, line, check_name, category, message, severity
- Persistence to findings table
- CLI: `rmap findings import <db> <repo> <report> --format clang-tidy`
- CLI: `rmap findings list <db> <repo>`

### Phase 2: Sanitizer Logs

**In Scope Phase 2:**
- ASan/UBSan/TSan log parser
- Stack trace extraction
- Finding attribution to functions
- CLI: `--format asan`, `--format ubsan`, `--format tsan`

### Phase 3: scan-build

**In Scope Phase 3:**
- scan-build plist parser
- HTML report directory parser
- CLI: `--format scan-build`

### Out of Scope

- Running tools (user runs, imports result)
- Fix application (clang-tidy --fix)
- Suppression management
- Finding deduplication across runs (later)

## Design

### Finding Model

```rust
pub struct AnalyzerFinding {
    pub snapshot_uid: String,
    pub tool: String,              // "clang-tidy", "asan", "ubsan", "scan-build"
    pub tool_version: Option<String>,
    pub check_name: String,        // "bugprone-not-null-terminated-result"
    pub category: Option<String>,  // "bugprone", "Memory error"
    pub severity: FindingSeverity, // Error, Warning, Note
    pub file_path: String,         // Repo-relative
    pub line: u32,
    pub column: Option<u32>,
    pub message: String,
    pub function_key: Option<String>, // If attributable to a function
}

pub enum FindingSeverity {
    Error,
    Warning,
    Note,
    Remark,
}
```

### clang-tidy Parser

```rust
pub fn parse_clang_tidy_output(output: &str) -> Vec<RawFinding> {
    // Parse text format:
    // /path/to/file.c:42:10: warning: message [check-name]
    let re = Regex::new(
        r"^(.+):(\d+):(\d+): (error|warning|note): (.+) \[([^\]]+)\]$"
    ).unwrap();
    
    output.lines()
        .filter_map(|line| {
            re.captures(line).map(|caps| RawFinding {
                file: caps[1].to_string(),
                line: caps[2].parse().unwrap(),
                column: caps[3].parse().ok(),
                severity: parse_severity(&caps[4]),
                message: caps[5].to_string(),
                check_name: caps[6].to_string(),
            })
        })
        .collect()
}
```

### Persistence

```sql
CREATE TABLE analyzer_findings (
    finding_uid TEXT PRIMARY KEY,
    snapshot_uid TEXT NOT NULL,
    tool TEXT NOT NULL,
    tool_version TEXT,
    check_name TEXT NOT NULL,
    category TEXT,
    severity TEXT NOT NULL,
    file_path TEXT NOT NULL,
    line INTEGER NOT NULL,
    column INTEGER,
    message TEXT NOT NULL,
    function_key TEXT,
    FOREIGN KEY (snapshot_uid) REFERENCES snapshots(snapshot_uid)
);

CREATE INDEX idx_findings_snapshot ON analyzer_findings(snapshot_uid);
CREATE INDEX idx_findings_file ON analyzer_findings(snapshot_uid, file_path);
CREATE INDEX idx_findings_function ON analyzer_findings(snapshot_uid, function_key);
```

### CLI

```bash
# Import clang-tidy output
rmap findings import ./repo.db repo-uid tidy-output.txt --format clang-tidy

# Import sanitizer log
rmap findings import ./repo.db repo-uid asan.log --format asan

# List findings
rmap findings list ./repo.db repo-uid

# List findings for a file
rmap findings list ./repo.db repo-uid --file src/main.c

# Findings summary
rmap findings summary ./repo.db repo-uid
# Output: N findings, M files affected, top categories

# Findings by severity
rmap findings list ./repo.db repo-uid --severity error
```

### Risk Integration

Findings should feed into risk scoring:
- File with sanitizer findings → elevated risk
- Function with clang-tidy bugprone findings → flag in orient output
- Hotspot + findings = high-priority attention

This is a separate integration slice, but the finding model should support it.

## Definition of Done (Phase 1)

- [ ] clang-tidy text output parser
- [ ] Finding model and storage schema
- [ ] Path normalization (absolute → repo-relative)
- [ ] `rmap findings import --format clang-tidy` command
- [ ] `rmap findings list` command
- [ ] `rmap findings summary` command
- [ ] Unit tests with sample clang-tidy output
- [ ] Integration test: run clang-tidy on a file, import, verify findings

## Test Plan

1. **Unit tests:**
   - Parse sample clang-tidy output
   - Handle various severity levels
   - Handle check names with hyphens

2. **Integration test:**
   ```bash
   # Run clang-tidy on test file
   clang-tidy test.c --checks='*' > tidy-output.txt
   
   # Import
   rmap findings import ./test.db test-repo tidy-output.txt --format clang-tidy
   
   # Verify
   rmap findings list ./test.db test-repo
   ```

3. **Real repo test:**
   - Run clang-tidy on a real C/C++ project
   - Import findings
   - Verify count and categories

## Dependencies

- `regex` crate for text parsing
- `plist` crate for Phase 3 (scan-build)
- Storage schema extension

## Risks

- Output format variations between clang-tidy versions
- Sanitizer output is less structured than clang-tidy
- Large projects may have many findings

Mitigation:
- Start with well-defined clang-tidy format
- Sanitizer parsing is Phase 2 (learn from Phase 1)
- Efficient storage and filtering
