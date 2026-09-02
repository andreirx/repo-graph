# CHECK-LANG-SPLIT-1 — mixed repos get per-language confidence, not a blend

Status: SPECIFIED (2026-09-02) · Track: v0.13.0 queue tail. CODE slice. Maturity: MATURE
(check/orient CI-facing; figures frozen).

## 1. Problem (measured — audit 2026-09-01T09-06-40Z)

glamCRM (TS+Java) and repo-graph (Rust+TS) render ONE blended reliability line — "your
code's calls 23% resolved (LOW)" — with a generic CTA. The blend hides per-language
confidence differences (TS post-enrichment may be far above Java's unresolved mass), and
the CTA cannot be per-language when the figure isn't. Single-language repos already get
their honest per-language lines; mixed repos — the operator's primary shape — get the
worst rendering.

## 2. Contract

1. **The blended figure stays** (it is the repo-level truth and the frozen verdict input);
   UNDER it, mixed repos (≥2 materially-present code languages) render a per-language
   breakdown line: "by language: TypeScript N% of M calls · Java N% of M calls" — computed
   from the SAME per-language reliability facts `rmap reliability --by-language` already
   serves (one source; no new computation — if the by-language facts are not reachable at
   check/orient's site without a new public API, cite the precedent chain or STOP).
2. **The CTA follows the split**: the per-language CTA logic (CS-1) already names languages;
   ensure the breakdown and the CTA agree (same materiality gate, same display names — the
   one-source rules already in force).
3. **Verdict contribution unchanged** (the blended figure remains the CALL_GRAPH_RELIABILITY
   input; the breakdown is informational). Exit codes and JSON shapes frozen; breakdown
   additive in JSON.
4. Single-language repos: no breakdown line (nothing to split — no noise).

## 3. Stop conditions

Frozen: reliability computation and figures, verdict mapping, exit codes, storage schema.
STANDING HONESTY RULES (a failed by-language read renders unknown-with-reason for the
breakdown line only — the blended line is independent). New public APIs beyond additive DTO
fields → DECISION_REQUIRED. Unmet DoD → STOP + DECISION_REQUIRED. Never touch the
operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit: breakdown for 2- and 3-language mixes; single-language suppression; agreement with
  the CTA's language set; failed by-language read → unknown-with-reason breakdown, blended
  line intact.
- Live proof (isolated state root, registry sha unchanged): glamCRM — blended line + "by
  language: TypeScript …% · Java …%" agreeing with `reliability --by-language`; leveldb —
  no breakdown line. Captures.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

A mixed repo's reader sees which language carries the risk; the breakdown, the CTA, and
reliability --by-language agree from one source; single-language output unchanged; verdict
untouched; gates green.
