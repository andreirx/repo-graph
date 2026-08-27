# ORIENT-SEGMENT-2 — orient survives manifest-poor topologies; budgets never change facts

Status: SPECIFIED (2026-08-28) · Track: Usefulness-audit fix queue #5
(`docs/ROADMAP.md` § Usefulness audit v0.9.0; evidence `smoke-runs/2026-08-25T22-41-37Z`).
CODE slice. Maturity: MATURE (the primary surface).

## 1. Problem (measured)

- **django's orient is anti-information** (OBSERVED): `1 package group: .` + two modules both
  rendered `Django` (87 vs 907 files, indistinguishable) at every budget; after `--full` (123
  lines) an agent knows 83 function names and zero subsystems — while `stats` on the same
  snapshot already holds the correct 685 directory groups with a true fan-in ranking
  (`django/db 340`, `django/test 242`, `django/core 195`). Orient chooses the useless Layer-0
  manifest view over a good Layer-1 view it already computed.
- **Budget changes FACTS** (OBSERVED, FRAKTAG): resolution reads 28% at `--budget large` and 31%
  at `--full`; call totals 1609 vs 1685 — the budget must change LENGTH, never numbers.
- **`--full` is a false promise** (OBSERVED): byte-identical to `--budget large` on 4/6 repos
  while the small-budget footer advertises "[--full for the complete breakdown]".
- **Headline gaps** (OBSERVED): the REST surface count — the architecture on glamCRM/amodx —
  never appears in orient; `.env.test` reaches amodx's Docs headline (docs-side fixed in
  SELF-POLLUTION-1; orient's line must not re-introduce it).
- **Module names collide** (OBSERVED): two manifests named `Django` render identically; amodx
  names differ across commands (`packages/plugins` vs `@amodx/plugins`).

## 2. Contract

1. **Topology fallback**: when the package-group view collapses (one group covering ≥90% of
   files, or ≥N files in a single group — thresholds stated in code with rationale), orient's
   structural section PROMOTES the directory-group view stats already computes (top groups by
   fan-in, honestly labeled "directory groups (no manifest topology at this depth)"). The
   manifest view stays available; nothing is recomputed differently — same facts, better choice.
2. **Module identity**: inferred/declared module rows render `name [manifest-path]` whenever two
   modules share a name or the name differs from the path (`Django [pyproject.toml]` /
   `Django [package.json]`); one naming across orient/modules/stats (the amodx divergence
   closes or is reported).
3. **Budgets change LENGTH only**: every number orient prints (resolution %, call totals,
   counts) derives from the same snapshot reads regardless of budget; a test runs orient at all
   four budgets on one fixture and asserts numeric identity (facts) with monotonic length.
4. **`--full` earns its name or says so**: where full == large today, either full renders the
   genuinely-complete sections (no elision) or the small-budget footer stops advertising it;
   choose the former where the data exists (it does: the elided lists). A saturated ladder
   (repo smaller than the budget) states "budget not reached — output complete".
5. **The HTTP surface count joins the headline** where >0 (one line: `244 HTTP surfaces
   (222 providers / 23 consumers) — rmap surfaces`), sourced from the HSC-1 union.
6. Orient's Docs line: never `.env*`, never generated files (consume SELF-POLLUTION-1's
   classifier; cap the list at the most orienting docs — README/architecture first, not
   alphabetical).

## 3. Stop conditions

Frozen: storage schema, stats computation (orient CONSUMES it), trust, LiveGraph/witness/union/
reconciliation, exit codes, the orient JSON envelope beyond additive fields. If the numeric-
identity test exposes a budget-dependent READ path (the FRAKTAG 28-vs-31 root cause), fixing it
must not change which read is authoritative without a finding — surface what differed. Never
touch the operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit: topology-collapse trigger (both thresholds + non-collapse negative); name-collision
  suffixing; the four-budget numeric-identity + monotonic-length test; saturated-ladder line;
  headline HTTP line; Docs-line filtering/capping.
- LIVE isolated proofs: django — orient names django's real subsystems (db/test/core/utils/…)
  with fan-in at every budget ≥ medium, modules distinguishable; FRAKTAG — identical numbers at
  large and full (root cause of 28-vs-31 named in the report); glamCRM/amodx — HTTP headline
  present, no `.env` in Docs; leveldb — unchanged except the saturated-ladder line (its orient
  is the gold standard — byte-minimal diff, shown in the report).
- Chunked cargo gates; witness green; dogfood green; logged smoke SMOKE_ONLY="django FRAKTAG" green.

## 5. Definition of done

Orient gives real structure on manifest-poor repos using facts it already holds, numbers that
never depend on the budget, a `--full` that means something, the HTTP architecture in the
headline, and a Docs line worth reading; leveldb's gold-standard output survives byte-nearly
unchanged; gates green.
