# DEAD-CAUSES-1 — dead's refusal states causes from the snapshot, not from April

Status: SPECIFIED (2026-08-28) · Track: Usefulness audit v0.9.0 fix queue, item #8. CODE slice,
small. Maturity: MATURE surface (the refusal is a ratified contract; its TEXT is the defect).

## 1. Problem (measured — audit §8)

`rmap dead` is DELIBERATELY DISABLED (ratified 2026-04-27; `rgr/src/commands/dead.rs:45` — that
decision is NOT re-litigated here; exit code 2 and the refusal itself are frozen). But the
refusal prints STATIC 2026-04 causes: "Missing framework detectors (Spring, React, Axum,
FastAPI)". VERIFIED today: `repo-index/src/react_detector.rs` exists and the audit snapshot
carries **212 React + 14 Spring liveness inferences** — the text claims machinery is missing
that demonstrably ran on the reader's own snapshot. A refusal that cites stale causes teaches
the reader a false model of the tool (name-vs-semantics defect on a whole surface), and it will
rot again the same way unless causes are DERIVED, not transcribed.

## 2. Contract

1. **The refusal stands; the causes become snapshot-derived.** `rmap dead` still refuses with
   exit code 2 and the same shape (what/why/alternatives/reintroduction). When the cwd resolves
   to an indexed repo, the "Root causes" section is computed from THAT snapshot:
   - Framework liveness: if framework inferences EXIST for the snapshot (per family, with
     counts), say what is true: "framework liveness inferences exist (React: N, Spring: M) but
     are not wired into deadness evidence"; a family with zero inferences AND no detector for a
     materially-present language is honestly "no <family> detector". Counts come from the
     stored inference facts — never from detector names or hardcoded lists that can go stale.
   - Coverage evidence: present/absent read from the snapshot's coverage facts.
   - Entrypoint declarations: present/absent read from whatever entrypoint facts exist; if no
     such fact class exists, the line says so generically — but then it must be phrased as a
     capability statement ("rmap records no entrypoint declarations"), not a repo claim.
2. **Unknown with reason, never stale-static-as-truth.** Daemon unreachable / read fails →
   "causes could not be derived for this directory (<reason>); generic causes follow" + the
   generic list explicitly labeled generic. No indexed snapshot → same labeling with the
   not-indexed reason. The static text may remain ONLY under that explicit label.
3. **No re-enable, no new verb.** `orphans`/reintroduction remain future work; the
   reintroduction-conditions lines are updated to the same derived honesty (e.g. coverage
   still absent → that condition stands verbatim).
4. Client-side: reuse existing daemon queries/dispatch arms where the facts are already served;
   a new dispatch fact-class arm (if one is genuinely needed for inference counts) is in scope
   with the witness-manifest line. No storage schema change.

## 3. Stop conditions

Frozen: the disable decision + exit code 2, storage schema, trust, LiveGraph/witness. If the
inference counts are not reachable without a NEW public API beyond a dispatch arm, STOP +
DECISION_REQUIRED. STANDING HONESTY RULES apply. Unmet DoD → STOP + DECISION_REQUIRED. Never
touch the operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit: derived causes for (a) snapshot with framework inferences (counts named, no "missing
  detector" claim for that family), (b) snapshot with none, (c) no snapshot for cwd (labeled
  generic), (d) daemon-unreachable (reason + labeled generic). Exit code 2 in all cases.
- Live proof (isolated state root, registry sha unchanged): glamCRM (React inferences present)
  → refusal names the real counts and does NOT claim a missing React detector; leveldb (no
  framework inferences) → honest absence lines. Captures in the report.
- Chunked cargo gates; consolidation witness (arm declared if added); dogfood-isolated green.

## 5. Definition of done

The refusal never claims absent machinery that the reader's own snapshot disproves; every
cause line is derived or explicitly labeled generic-with-reason; the disable decision and exit
semantics are untouched; gates green.
