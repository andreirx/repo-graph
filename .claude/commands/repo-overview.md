Index a codebase with rgr and produce a structural health overview.

Usage: /repo-overview <path>

Steps:
1. Run `rgr repo add $ARGUMENTS` to register the repo (skip if already registered)
2. Run `rgr repo index <name>` to index (or re-index) the codebase
3. Run `rgr graph stats <name> --json` for module structural metrics
4. Run `rgr graph cycles <name> --json` for dependency cycles
5. Run `rgr arch violations <name> --json` for boundary violations (if boundaries declared)
6. Run `rgr graph metrics <name> --module --json` for per-module complexity
7. Run `rgr graph metrics <name> --limit 10 --json` for top complex functions
8. Run `rgr graph dead <name> --kind SYMBOL --json` for dead code candidates
9. Produce a summary: module count, cycle count, violation count, top complex modules, top complex functions, dead code count
