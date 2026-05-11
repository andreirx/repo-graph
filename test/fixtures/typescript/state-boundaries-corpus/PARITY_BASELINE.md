# SB-7A Parity Baseline

Generated: 2026-05-11
Commit: post-SB-7A multi-language emitter fix

## Edge Counts
- READS: 7
- WRITES: 3
- Total state-boundary edges: 10

## Edge Tuples (type|source_key|target_key|resolution)

```
READS|state-boundaries-corpus:src/fs-promises.ts#loadAsync:SYMBOL:FUNCTION|state-boundaries-corpus:fs:/async/config.yaml:FS_PATH|static
READS|state-boundaries-corpus:src/fs-read.ts#loadConfig:SYMBOL:FUNCTION|state-boundaries-corpus:fs:/etc/app.yaml:FS_PATH|static
READS|state-boundaries-corpus:src/fs-read.ts#loadMultiple:SYMBOL:FUNCTION|state-boundaries-corpus:fs:/etc/cache.conf:FS_PATH|static
READS|state-boundaries-corpus:src/fs-read.ts#loadMultiple:SYMBOL:FUNCTION|state-boundaries-corpus:fs:/etc/db.conf:FS_PATH|static
READS|state-boundaries-corpus:src/node-fs.ts#loadData:SYMBOL:FUNCTION|state-boundaries-corpus:fs:/data/input.json:FS_PATH|static
READS|state-boundaries-corpus:src/uri-paths.ts#loadFromUri:SYMBOL:FUNCTION|state-boundaries-corpus:fs:file:///etc/ssl/certs/ca-bundle.crt:FS_PATH|static
READS|state-boundaries-corpus:src/uri-paths.ts#loadWindowsPath:SYMBOL:FUNCTION|state-boundaries-corpus:fs:C:\Windows\System32\config:FS_PATH|static
WRITES|state-boundaries-corpus:src/fs-promises.ts#saveAsync:SYMBOL:FUNCTION|state-boundaries-corpus:fs:/async/output.txt:FS_PATH|static
WRITES|state-boundaries-corpus:src/fs-write.ts#saveConfig:SYMBOL:FUNCTION|state-boundaries-corpus:fs:/var/log/app.log:FS_PATH|static
WRITES|state-boundaries-corpus:src/node-fs.ts#saveData:SYMBOL:FUNCTION|state-boundaries-corpus:fs:/data/output.json:FS_PATH|static
```

## Evidence Validation

### file:// URL (src/uri-paths.ts#loadFromUri)
```json
{
  "logical_name_source": "normalized_url"
}
```

### Windows path (src/uri-paths.ts#loadWindowsPath)
```json
{
  "logical_name_source": "normalized_path"
}
```

## Coverage
- [x] FS read (named import from "fs")
- [x] FS write (named import from "fs")
- [x] node:fs module specifier
- [x] node:fs/promises module specifier
- [x] Non-FS negative case (no state-boundary edges from non-fs.ts)
- [x] URI-shaped resource (file:// URL → normalized_url)
- [x] Path-shaped resource (Windows path → normalized_path)

## Parity Commands (CLI-only)

### Quick parity check

```bash
rmap index test/fixtures/typescript/state-boundaries-corpus /tmp/sb-parity.db
# Expected: indexed 6 files, 28 nodes, 16 edges

rmap resource list /tmp/sb-parity.db state-boundaries-corpus
# Expected: count=10, total_reads=7, total_writes=3
```

### Detailed validation

```bash
rmap resource readers /tmp/sb-parity.db state-boundaries-corpus \
  "state-boundaries-corpus:fs:/etc/app.yaml:FS_PATH"
# Expected: count=1, source=loadConfig

rmap resource writers /tmp/sb-parity.db state-boundaries-corpus \
  "state-boundaries-corpus:fs:/var/log/app.log:FS_PATH"
# Expected: count=1, source=saveConfig
```

### Negative case

```bash
rmap resource list /tmp/sb-parity.db state-boundaries-corpus --kind DB_RESOURCE
# Expected: count=0 (no DB resources in corpus)
```

Diff against this baseline to validate future refactors.
