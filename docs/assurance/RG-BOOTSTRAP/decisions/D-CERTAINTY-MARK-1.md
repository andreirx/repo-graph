# D-CERTAINTY-MARK-1 — What does the product do with a fact the index cannot determine?

Raised: 2026-09-23 by the manager after audit round seven's fix queue hit the same wall in four slices: CPP-ATTRIBUTE-MACRO-1 (five document reviews — the macro-recovery parse tree is symmetric, `void MYAPI() foo() {…}` ≡ `void Lock() EXCLUSIVE_LOCK_FUNCTION() {…}`), PYTHON-SUBMODULE-IMPORT-1 (Python binds `from X import Y` at run time; the extractor cannot prove `X/__init__.py` does not bind `Y`), PYTHON-RECEIVER-BINDING-1 (a unique method name on an untyped receiver), CPP-INCLUDE-BASENAME-1 (a unique basename).
Resolved: 2026-09-23 by the HUMAN — "anything hard to determine from our indexes we explicitly mark for the agents to investigate themselves."

## The problem

Each slice had three ways out, all wrong under the catalog as it stood: guess (pick one candidate and print it as a certain fact — the fabrication class every audit has fought), withhold (emit nothing — an RG-REQ-001-L01 completeness exception the reviewers refused), or defer (leave the measured defect). The catalog had no fourth way: mark the fact as undetermined or inferred with its candidates and reason, and hand it to the agent.

## Options

- **Guess** — pick one candidate and print it as a certain fact (today's behaviour in every case above). Reward: the surfaces stay dense and the counts high. Risk: the fabrication class every audit has fought — an agent acts on a relationship that is not there.
- **Withhold / unresolved-only** — emit nothing for the ambiguous fact, or an unresolved row with no candidates. Reward: no false fact. Risk: a genuine declaration or binding disappears (an RG-REQ-001-L01 completeness exception the reviewers refused five times); the agent is not told there was something to look at.
- **Mark (ratified)** — store the fact once with every candidate and its reason, flagged `inferred` (edge) or `identity: undetermined` (node); default views count certain facts, the remainder is stated with a flag; the agent investigates. Reward: nothing is lost, nothing is asserted, the agent knows exactly where to look. Risk: every aggregating surface must partition by certainty (RG-REQ-004-L12 and this L define the composition), and a stale consumer that ignores `resolution` would count inferred edges — guarded by the per-slice exclusion tests.

## Resolution

RG-REQ-002-L11 (the human's words) plus cross-references in RG-REQ-001-L01, RG-REQ-005-L02, RG-REQ-006-L04 (Python clause) and L11, RG-REQ-008-L08. Mechanics: the existing `edges.resolution = inferred` for edges; additive `metadata_json` keys (`identity: undetermined`, `identity_candidates`, `basis`) for nodes; aggregating surfaces show certain facts by default and state the inferred remainder with a flag — the same partition pattern as RG-REQ-004-L12. Consequence for the parked slices: no withholding, no guessing; each re-cut adds the renderer path for its "investigate" line (a real vertical). D-CAM-01's R1–R4 mechanics are superseded by this rule (the symmetric member is marked with both candidates; the member-argument field rule stays).
