Produce a full structural and quality health report for an indexed codebase.

Usage: /assess-health <repo>

Prerequisites: repo must be indexed. For full report, run `rgr graph churn <repo>` and `rgr graph coverage <repo> <report>` first.

Steps:
1. Run `rgr graph stats $ARGUMENTS --json` for module structural metrics (instability, abstractness)
2. Run `rgr graph cycles $ARGUMENTS --json` for dependency cycles
3. Run `rgr arch violations $ARGUMENTS --json` for boundary compliance
4. Run `rgr graph metrics $ARGUMENTS --module --json` for per-module complexity aggregates
5. Run `rgr graph metrics $ARGUMENTS --limit 10 --json` for top complex functions
6. Run `rgr graph hotspots $ARGUMENTS --limit 10 --json` for churn x complexity hotspots (if churn imported)
7. Run `rgr graph risk $ARGUMENTS --limit 10 --json` for under-tested hotspots (if coverage imported)
8. Run `rgr graph versions $ARGUMENTS --json` for domain version info
9. Produce a health report with sections: Structure, Complexity, Hotspots, Risk, Version
10. Flag specific concerns: high instability modules, cycles, violations, CC > 15 functions, uncovered hotspots
