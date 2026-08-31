# TRUST-FIRSTPARTY-1 — the repo's own crates are not "external libraries"

Status: SPECIFIED (2026-09-01) · Track: Usefulness audit v0.11.0 fix queue, tail item.
CODE slice, small. Maturity: MATURE (trust).

## 1. Problem (measured — audit run 2026-08-30T20-44-57Z, repo-graph)

trust renders "library call → repo_graph_daemon_transport: 1074 references … follow to that
dependency's crate / package docs" (likewise repo_graph_storage, repo_graph_agent, …) and
"14% of calls go into external libraries" — inflated by the repo's OWN workspace crates. The
CTA sends an orienting agent to crates.io for code that lives in this repo.

## 2. Contract

1. **First-party classification from manifest facts**: a callee family whose package name
   matches a manifest THIS repo's index parsed (workspace members / declared packages — the
   parsed-manifest facts DEPS-ATTRIB-2 hardened; NEVER name-prefix heuristics) classifies as
   FIRST-PARTY: rendered as "internal crate/package → <name> (this repo)" with the honest
   next move (`rmap explain <name>` / the module surface), excluded from the
   external-libraries percentage. The externality name-set for TRUE externals is unchanged.
2. **Unknown stays unknown**: a family matching neither the parsed-manifest set nor the
   external name-set keeps its current unclassified handling (no new claims).
3. **Figures recompute honestly**: the external % excludes first-party; the basis: line
   (CONTRADICTION-SWEEP-1 pattern) states the split ("N external, M internal workspace
   references").
4. JSON additive (first_party marker/bucket); exit codes unchanged; trust ratio/denominator
   semantics untouched (this is CLASSIFICATION of the external bucket, not the trust
   computation — if the two cannot be separated, STOP + DECISION_REQUIRED).

## 3. Stop conditions

Frozen: trust ratio/denominator computation, storage schema, exit codes. STANDING HONESTY
RULES. New public APIs beyond additive DTO fields → DECISION_REQUIRED. Unmet DoD → STOP +
DECISION_REQUIRED. Never touch the operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit: first-party classification from parsed-manifest facts (fixture workspace); unknown
  unchanged; percentage split; basis line.
- Live proof (isolated state root, registry sha unchanged): repo-graph — its workspace
  crates render as internal with the in-repo next move, external % drops accordingly (record
  before/after); glamCRM spot-check (its @glamcrm/* workspace packages if referenced).
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

No agent is sent to crates.io for this repo's own code; first-party references are labeled
internal with an in-repo next move; the external figure means external; classification
derives from parsed manifests only; gates green.
