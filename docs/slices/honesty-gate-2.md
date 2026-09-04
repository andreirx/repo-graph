# HONESTY-GATE-2 — resources, trust, map never assert what they cannot know

Status: SPECIFIED (2026-09-04) · Track: v0.16.0 audit queue #2 (human-ratified). CODE
slice, diagnose-then-fix per family. Maturity: MATURE.

## 1. Problem (VERIFIED — docs/audits/2026-09-04-per-command-usefulness-v0.16.0.md)

Three surfaces emit rows no evidence supports — the fabrication class:
- **resource list (computed path + invented modes):** vscode `.env FS_PATH 2 readers 2
  writers` — NO literal-path fs call to `.env` exists; the only sources are
  `join(dir,'.env')` in a test, `resolve(__dirname,'.env')` (not an fs call), and an
  array-literal string in a test. The row violates the literal-path caveat printed six
  lines above it. hadoop `. FS_PATH 6 readers 5 writers` — the 6 readers are exactly
  right (all `O_RDONLY`); the 5 writers are invented.
- **trust (name-shaped framework detection):** hadoop `nextjs_app_router_detected` — no
  `next` dependency, no `next.config.*`; the `src/app/` directory is React Router 7. A
  directory NAME drove a framework claim that then downgraded reliability.
- **map --dry-run (invented resolution):** hadoop `.mvn/wrapper/MavenWrapperDownloader.java`
  shown with 3 resolved intra-repo edges while its own file map in the same artifact reads
  `External / unresolved (11)` and it imports only JDK classes.

## 2. Contract — the invariant, applied per family

**INVARIANT: no row is emitted whose evidence the printed caveat excludes.** Diagnose each
family's mechanism on the real corpus FIRST (DB rows + extractor provenance), then:
1. **Resources:** a resource row requires a string-literal path in the ACCESS CALL's
   path-argument position (arg0 of the detected fs/db call) — a literal appearing as an
   argument to a path-join/resolve helper, an array element, or any non-access call is
   NOT evidence. Access MODE (reader/writer) is emitted only from evidence the detector
   actually has (flags/mode argument/method name); undetermined mode renders as
   "access (mode unknown)" — never a guessed writer count. Test-file resources follow the
   existing is_test labeling.
2. **Trust framework detections:** a framework claim requires structural evidence
   (dependency manifest entry, config file, or framework import) — a directory or file
   NAME alone never detects a framework (STANDING RULE). The detection basis is rendered
   with the claim ("nextjs: next@14 in package.json + next.config.js"). Absent basis → no
   claim, and no reliability downgrade derived from it.
3. **Map resolution:** an intra-repo resolved edge requires the resolver to have matched
   a repo symbol by qualified identity, not by bare-name collision with a JDK/std class
   (`File`, `Path`, …). Where a collision is the only match, the edge is unresolved
   (external/unknown) — the file map and the edge list must agree within one artifact.
4. JSON additive (basis fields); exit codes unchanged; the three verified fabrications
   are gone in the live proofs; no NEW suppression of correctly-evidenced rows (hadoop's
   6 readers must survive exactly).

## 3. Stop conditions

Frozen: detector/extractor output shapes (additive), storage schema (additive), exit
codes. Widening any detector's RECALL is out of scope (RESOURCE-DYNAMIC-PATH-1 etc.) —
this slice only removes unsupported claims and states bases. If a family's true cause
requires machinery beyond a gate/basis check, STOP + DECISION_REQUIRED for that family
(ship the others). STANDING HONESTY RULES. Unmet DoD → STOP + DECISION_REQUIRED. Never
touch the operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Reproducing tests FIRST per family: literal-in-join → no resource row; mode-unknown →
  no writer count; dir-name-only → no framework claim; JDK-collision → unresolved edge.
- Live proof (isolated state root, registry sha unchanged): vscode resource list (.env
  gone; any survivors carry access-position evidence), hadoop resource list (6 readers
  survive; writers gone or evidenced), hadoop trust (no nextjs; basis rendered on any
  real detection elsewhere — e.g. amodx/glamCRM real frameworks still detected WITH
  basis), hadoop map (wrapper file's edges agree with its file map). Before/after
  verbatim; non-affected repos byte-stable.
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

Every resource row, framework claim, and resolved edge carries evidence the caveat
admits; the three verified fabrications are gone while correctly-evidenced rows survive
exactly; bases render with claims; gates green.

CORPUS PATHS: vscode at ../legacy-codebases/vscode; hadoop at ../legacy-codebases/hadoop;
amodx at ../amodx; glamCRM at ../glamCRM; repo-graph is THIS repo.
