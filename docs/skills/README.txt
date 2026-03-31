Agent Skills
=============

A skill is a reusable workflow that teaches an AI agent HOW to use this tool's
CLI commands to accomplish a specific analysis task.

Skills are NOT code. They are structured instructions.
The tool stays deterministic. The skill encodes the engineering judgment.


WHAT A SKILL CONTAINS
---------------------

Each skill file has:

1. Purpose      — What question does this skill answer?
2. Prerequisites — What must be true before running (indexed repo, declarations, etc.)
3. Steps        — The exact CLI commands to run, in order
4. Interpretation — How to read the results and make decisions
5. Decision Logic — If X, then do Y. If Z, stop and report.
6. Output       — What the agent should produce at the end


WHY SKILLS EXIST
----------------

An agent with access to the CLI can run any command. But knowing WHICH commands
to run, in WHAT order, and HOW to interpret the results requires engineering
knowledge. Skills compress that knowledge into reusable recipes.

Without skills, the agent must improvise the analysis workflow every time.
With skills, the agent follows a proven sequence and focuses its reasoning
on interpreting results rather than inventing process.


HOW AN AGENT USES A SKILL
--------------------------

1. The agent reads the skill file.
2. The agent executes the CLI commands in order, passing --json.
3. The agent reads the JSON output and applies the decision logic.
4. The agent produces the specified output.

The agent can deviate from the skill if the results warrant it, but the skill
provides the default path.


SKILL INVENTORY (PLANNED)
--------------------------

v1 skills (use only v1 CLI commands):

  investigate-symbol.txt
    "I need to understand what this symbol does and who depends on it."
    Commands: graph callees, graph callers, graph path

  find-dead-code.txt
    "Find unreachable code in this repo."
    Commands: declare entrypoint (verify), graph dead

  detect-circular-deps.txt
    "Find and report circular dependencies."
    Commands: graph cycles, graph path (for each cycle, show the chain)

  verify-declarations.txt
    "Check that declared boundaries match the extracted graph."
    Commands: declare list, graph imports (for each boundary, check for violations)

v2 skills (use v2 CLI commands):

  investigate-change-impact.txt
    "I need to modify symbol X. What else could break?"
    Commands: change impact, change pinch-points, change required-tests

  characterize-legacy-method.txt
    "I need to test a legacy method before modifying it."
    Commands: legacy seams, legacy characterize, change uncovered

  find-architecture-violations.txt
    "Show me all dependency rule violations in the codebase."
    Commands: arch violations, arch boundaries, declare boundary

  trace-data-flow.txt
    "Trace how data flows from this HTTP endpoint to the database."
    Commands: flow trace, flow data-path, arch sinks

  assess-test-coverage-gaps.txt
    "Which critical code has no tests?"
    Commands: legacy hotspots, change uncovered, legacy monster-methods

  detect-dead-code-safe.txt
    "Find dead code with dynamic-reference safety checks."
    Commands: legacy dead-code-candidates (extends v1 graph dead)

  plan-safe-refactoring.txt
    "I want to refactor this hotspot. What's the safe approach?"
    Commands: legacy hotspots, legacy seams, change impact,
    change required-tests, legacy characterize

  extract-state-machine.txt
    "Map the implicit state machine for this entity."
    Commands: legacy state-machine, graph callers (for each transition)

  audit-error-handling.txt
    "Find silent exception swallowing and missing error handling."
    Commands: legacy error-flows, graph path (trace from throw to catch)

v3 skills (use fleet commands):

  map-system-topology.txt
    "Show me how all services connect."
    Commands: fleet map, fleet deps, fleet event-topology

  check-api-compatibility.txt
    "Will this API change break any consumer?"
    Commands: fleet api-drift, change contracts

  assess-migration-order.txt
    "Plan the order for strangling legacy services."
    Commands: fleet migration-order, fleet db-shared, fleet blast-radius


SKILL FORMAT
------------

Each skill file uses this structure:

  SKILL: <name>
  PURPOSE: <one-line question this skill answers>
  PHASE: v1 | v2 | v3
  PREREQUISITES:
    - <what must be true before running>
  STEPS:
    1. Run: <exact CLI command>
       Read: <what to look for in the output>
       Decide: <decision logic>
    2. Run: <next command, possibly using output from step 1>
       ...
  OUTPUT:
    <what the agent should produce at the end>
  NOTES:
    <edge cases, warnings, limitations>
