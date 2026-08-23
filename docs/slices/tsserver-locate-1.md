# TSSERVER-LOCATE-1 — find tsserver where TS projects actually keep it (nested `node_modules`)

Status: SPECIFIED (2026-08-23) · Track: Resolution & attribution (field finding, usefulness run
`smoke-runs/2026-08-23T18-38-37Z`). CODE slice, small. Maturity: MATURE (enrich lifecycle shipped).

## 1. Problem (measured)

glamCRM — the operator's primary TS target, ported to TS for this purpose — indexes with
**enrichment skipped: "tsserver not found — install typescript so tsserver is on PATH"**
(`doctor` capture) and reads **"your code's calls 23% resolved (LOW)"** on every surface. But the
repo HAS tsserver: `frontend/web/node_modules/.bin/tsserver` (and `frontend/workspace/`,
`serverless/`). OBSERVED: both the toolchain probe (`daemon-runtime/src/enrich_pass.rs:252`:
`binary_on_path("tsserver") || repo_root.join("node_modules/.bin/tsserver").is_file()`) and the
resolver's `find_tsserver` (`tsserver-resolver/src/client.rs:1073-1099`) look ONLY at the repo
root's `node_modules` or PATH. Any polyglot/monorepo layout whose TS packages live below the root
— the common shape, and the deployment target's shape — never gets the TS semantic witness, and
the honest skip message sends the reader to install a global typescript they do not need. amodx
and FRAKTAG (root `node_modules`) enrich fine in the same run — the gap is location, not the resolver.

## 2. Contract

1. **One locator, used by both the probe and the resolver.** For each TS project context the
   resolver already discovers (tsconfig/jsconfig roots — `tsserver-resolver/src/project.rs`),
   resolve `tsserver` by walking UP from the context directory to the repo root, first
   `node_modules/.bin/tsserver` wins; then the config-specified path; then PATH. The enrich-pass
   probe calls the SAME function over the discovered contexts (no second parallel heuristic), so
   "skipped" is only said when NO context can find one.
2. **Partial availability is per-context, not all-or-nothing.** Contexts with a tsserver enrich;
   contexts without are skipped and NAMED in the reader's language on `doctor` (and wherever the
   enrichment skip renders): "no typescript in `frontend/legacy` — `npm i -D typescript` there",
   not "install typescript globally".
3. **Byte-parity** on repos with a root `node_modules` or PATH tsserver (amodx, FRAKTAG): same
   binary chosen, same enrichment output.
4. No change to promotion semantics, trust denominator, or the single-pass doctrine.

## 3. Stop conditions

Frozen: promotion path, trust ratio, enrich_pass single-pass/cancellation doctrine, storage schema.
If project-context discovery does not expose the context directory needed for the walk-up, that is
a FINDING (extend minimally, additive) — do not rewrite project discovery. Never touch the
operator's real state root (isolated `/tmp` state roots only). Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Locator unit tests: nested `node_modules` (nearest wins over a farther one), root
  `node_modules`, PATH-only, none (→ honest per-context skip naming the directory).
- **Live lift proof on glamCRM** under an ISOLATED state root (sha256 the real
  `~/Library/Application Support/repo-graph/registry.json` before/after — unchanged): enrichment
  RUNS (doctor line), `rmap reliability --by-language` TypeScript figure RISES above the 23%
  baseline from `smoke-runs/2026-08-23T18-38-37Z/glamCRM-trust.txt`; report the before/after.
- Byte-parity on amodx or FRAKTAG enrichment output vs the same smoke run.
- Chunked cargo gates (standing pattern); consolidation witness 15/15; `./scripts/dogfood-isolated.sh` green.

## 5. Definition of done

glamCRM (and any nested-package TS repo) gets the TS semantic witness without a global install;
skips are per-context and name the directory; parity holds elsewhere; gates green.
