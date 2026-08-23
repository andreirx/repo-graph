# FORGET-REPO-1 — `repo remove` forgets; the daemon tracks and reclaims orphans

Status: SPECIFIED (2026-08-23) · Track: Distribution / daemon lifecycle (field finding on the
operator's own machine; operator-authored spec). CODE slice. Maturity: MATURE (registry +
state-root lifecycle are shipped contracts).

## 1. Problem (measured, 2026-08-23, operator state root)

`~/Library/Application Support/repo-graph/databases/` holds **53 `.db` files (13 GB)** while
`registry.json` references **25** → **28 orphan DB files (3.9 GB)** no command can see or
reclaim; **8 registered entries point at paths that no longer exist** (`/private/tmp/test_repo`,
`…/T/.tmpXXXX/test-repo`, …). The only de-index verb, `rmap repo remove <repo> [--delete-db]`
(`daemon-runtime/src/dispatch.rs:798-864`, REG-1), is a thin registry op with leaks, all
OBSERVED in code:

- Without `--delete-db` (the default) the whole index stays on disk forever; re-indexing the same
  path mints a NEW `repo_uid` into the SAME file (`registry.rs:60-71`, `db_path` = hash of path)
  → permanently unreachable rows no retention class can prune (retention keeps `current` by
  ratified design — it is not a forget mechanism).
- `--delete-db` unlinks only the base file: `-wal`/`-shm` sidecars survive (`dispatch.rs:854`).
- Unlink failure is swallowed (`.is_ok()`) and the CLI prints "Database retained" with no reason.
- In-memory eviction is gated on `RepoKey::new(db_path…)` canonicalizing, i.e. on the DB file
  still existing; `db_runtimes` slot never dropped (`state.rs:703-706`).
- `<repo>/.rgr/` (warm cache + livegraph-compare sidecars) is never removed by anything; after
  remove+reindex every warm-cache entry is a permanent `KeyMismatch`.
- No orphan detection anywhere (doctor, startup reconciliation, maintenance); zero tests on the path.

## 2. Contract

1. **`rmap repo remove <repo>` FORGETS by default** (decision recorded: "forget" must forget —
   supersedes REG-1's keep-by-default; `--delete-db` stays accepted as a no-op for muscle memory;
   new `--keep-db` opts out and prints where the DB stays). Forget = registry entry + in-memory
   state (eviction keyed on the registry entry, NOT on the DB file existing) + `db_runtimes` slot
   + `.db` + `-wal` + `-shm` + `<repo>/.rgr/` (only if the repo path exists). Each artifact is
   reported `removed | absent | failed(<io reason>)`; any `failed` → non-zero exit, honest line.
   Refuse with a clear error (no partial deletion) while the repo has an in-flight write op
   (`Writing`/`Refreshing` on its coordinator) — "index in progress; cancel it first".
2. **Orphan tracking in the daemon.** Startup reconciliation (`reconcile.rs`) and `rmap doctor`
   compute, from a directory listing (cheap): (a) DB files + sidecars in `databases/` not
   referenced by the registry (count, bytes); (b) registry entries whose `canonical_path` no
   longer exists; (c) sidecars without a base `.db`. `doctor` renders each class with a concrete
   next action (`rmap maintenance gc`; `rmap repo remove <path>`); the daemon log records the
   counts at boot. Unknown is never zero: if the listing fails, say so.
3. **`rmap maintenance gc [--dry-run] [--json]`** removes orphan DB files + stray sidecars
   (class a, c) and reports reclaimed bytes; dead-path registry entries (class b) are LISTED
   with the exact `rmap repo remove <path>` next action, NOT auto-removed (a path may be a
   temporarily unmounted volume — conservative). `--dry-run` lists without deleting.
4. **No retention-class change.** `classify.rs`, prune, the keep-set and W-B window are untouched.

## 3. Stop conditions

Frozen: retention classes, storage write schema, witness/union/reconciliation, trust. If forgetting
safely requires more than the coordinator check in §2.1 (e.g. a new lock), STOP + DECISION_REQUIRED.
Never touch the operator's real state root / registry (isolated `/tmp` state roots only). Do NOT
commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Tests (the path had none): remove → registry gone, memory evicted, slot dropped, `.db`+`-wal`+`-shm`
  gone, `.rgr/` gone; `--keep-db` keeps the file and says where; simulated unlink failure →
  `failed(reason)` + non-zero exit; remove during an in-flight write → refused, nothing deleted;
  evict still happens when the DB file was deleted out-of-band.
- Orphan detection: seeded isolated state root with an orphan `.db`, a stray `-wal`, a dead-path
  entry → `doctor` renders all three classes with bytes + next actions; `maintenance gc --dry-run`
  lists; `gc` removes a+c, reports bytes, leaves b listed; startup log shows counts.
- Live proof on an ISOLATED state root replaying the operator's shape (copy a few real `.db`
  files + a registry with a dead path): before/after `doctor` + `gc` captures in the report.
- Chunked cargo gates (standing pattern); consolidation witness 15/15; `./scripts/dogfood-isolated.sh`
  green; `rmap --help`/usage text updated (Protocol Surface Standard: naming tells the agent it is
  destructive).

## 5. Definition of done

"Forget repo X" is one honest command that removes everything repo-graph created for X and says
what it removed; the daemon sees and reports orphans on every boot and in `doctor`; `maintenance gc`
reclaims them; tests cover the whole path; gates green.
