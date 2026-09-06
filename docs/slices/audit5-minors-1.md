# AUDIT5-MINORS-1 — six one-file causes from audit round five (closed list)

Status: SPECIFIED (2026-09-06) · Track: audit round five, group F (human-ratified 2026-09-06).
CODE slice; each item is one cause in one file, listed closed. Maturity: MATURE surfaces.

## 1. Problem (ROOT-CAUSED — `docs/audits/2026-09-06-root-causes-v0.17.0.md` §F/§G)

Outward surface named per item. Six independent causes; none is a regression.

| # | Surface a user sees | Cause (file:line) |
|---|---|---|
| F1 | `docs list` labels `docs/monitoring/docker-compose.yaml` and `CHANGELOG.md` "architecture"; hadoop labels the same shapes "license"; repo-graph "architecture 577" | `doc-facts/src/classification.rs:47-112` is path-only and ordered: `docs/` in the path (:77) wins over the docker-compose/.yaml Config rule (:95); every other `.md` falls through to Architecture (:105-107) — the default bucket; `lib.rs:261-268` upgrades to License whenever the first 2000 chars contain "apache license" (an ASF HEADER, not a license document) |
| F2 | `boundaries list` prints `ximagepool.c ×13` with no line on any of 772 rows; `boundaries summary` none | storage HAS a line per hit (migration_024:43-46); the DTO carries `lineStart` (boundaries_list/mod.rs:53-59); the grouped human renderer (`group.rs:145-160`) never reads it — ANCHORS-EVERYWHERE-1 excluded grouped headlines ("never pick one line"), and the human list has NO ungrouped mode, so the anchor landed only in JSON/`boundaries show` |
| F3 | `callers <unknown>` not-found fallback spends 47% of its bytes on full `(cd … && rmap explain …)` cursors; seed rows repeat `model minishlab/potion-code-16M-v2` (17.7%) | `seed.rs:99-105` passes `repo_uid None` on the not-found path so `render_seed_chunk_candidate` cannot compose a short cursor; `seed.rs:215` puts `{source}, model {model}` on every row |
| F4 | `trust` opens with `Posture: Unavailable (Unavailable)` / `Resident: no` on 28/28 repos — a line that reads the same everywhere | root posture = MEET over the LiveGraph half (`trust_coherence.rs:43`; D-T6), which is resident only after the hidden `rmap dev livegraph-refresh` (TS-only, in-memory, lost on restart) — a constant carrying no information about the repo (`trust.rs:128-136, 227-242`) |
| F5 | `doctor` prints `[ok] vector store: unavailable` and `28/28 checks` green | `doctor/seed.rs:27-31,138` returns `passed: true` for every seed state; tone follows `passed`; the only `[note]` override exists for enrichment (`summary.rs:131`), none for seeds |
| F6 | smoke `00-meta.json` lists linux in `legacy_repos` but in none of passed/failed/skipped | `scripts/smoke-validation-repos.sh:301-312` drops `SMOKE_SKIP` repos from the run arrays without appending to `SKIPPED_REPOS`; `LEGACY_JSON` (:521-524) is built from the pre-filter list |

## 2. Contract

- **F1** Config rule (extension) precedes the `docs/` path rule; `.md` fallthrough becomes a
  neutral kind (`doc`), not `architecture` — "architecture" only by explicit name/dir; License
  only when the document IS a license (`LICENSE*`/`COPYING*`/`NOTICE*` names or marker-and-
  nothing-else), otherwise the header is ignored for kind. Movement: vscode/hadoop/repo-graph
  "By kind" blocks before/after; the repo-graph count of `architecture` drops from 577 to the
  explicitly named set.
- **F2** Grouped rows carry the SET of lines: `ximagepool.c ×13 @ 112,140,163,… (+8 more)`,
  capped at 5 shown — a set, never one picked line (consistent with ANCHORS-EVERYWHERE-1's
  rule). Summary stays line-free.
- **F3** The not-found fallback prints the same one-line cursor pattern header `find` prints
  and passes `Some(uid)` (uid from the candidates' stable-key prefix or added to the error
  data); the provenance triple `{source}, model {model}` is hoisted into the seeds heading
  and repeated per row ONLY when a row's model differs from the heading's (the honesty rule
  "a foreign-daemon row keeps its own label" survives). Measure: cursor-line share of
  `cr1-callers-nf` ≤ 15%; model string once on `eco-find-seeds`.
- **F4** The human `trust` render drops the LiveGraph posture lines; the JSON keeps D-T6's
  MEET unchanged. The snapshot posture (Fresh/Stale/…) is the headline. Movement: every
  smoke trust output loses the constant lines and gains nothing false.
- **F5** Seed probe tone is `[note]` when `state ∈ {unavailable, absent, degraded}` (mirror
  `apply_degraded_enrichment_tone`); `doctor` still passes (seeding is optional) but the
  count line does not read "28/28 checks" beside an unavailable store — it says
  `27 ok · 1 note`.
- **F6** `SMOKE_SKIP` repos land in `skipped` with `skip_reason: env`; the script's summary
  asserts `passed ∪ failed ∪ skipped == internal ∪ legacy`.

## 3. Stop conditions

Closed list — nothing outside F1–F6. Frozen: wire protocol, exit codes, storage schema,
D-T6's JSON semantics, the docs kind ENUM values beyond the one neutral addition (if `doc`
needs a new enum variant crossing the wire, it is ADDITIVE and stated). STANDING HONESTY
RULES. Unmet DoD → STOP + DECISION_REQUIRED. Never touch the operator's real state root.
Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

Unit per item FIRST: `classification.rs` (`docs/x/docker-compose.yaml → Config`; ASF-headed
CONTRIBUTING.md not License; plain README stays readme); `boundaries_list/tests.rs` ×3 test
gains `@ l1,l2,l3`; `seed.rs` tests :448/:653/:724-764 updated to the header pattern;
`trust.rs` render test without the LiveGraph lines; `doctor/mod.rs` tone test; the smoke
script dry-run `SMOKE_SKIP=linux SMOKE_ONLY="linux leveldb"` → `passed:[leveldb],
skipped:[linux]`. Live proof (isolated): the §2 measurements on vscode/hadoop/repo-graph
(docs), gstreamer (boundaries — spot-check three lines against source), repo-graph
(callers not-found, seeds, trust, doctor). Gates recorded FIRST; chunked cargo; witness;
dogfood-isolated.

## 5. Definition of done

All six measurements above hold; nothing outside the closed list changed; gates green.

CORPUS PATHS: vscode, hadoop, gstreamer at ../legacy-codebases/<name>; repo-graph is THIS repo.
