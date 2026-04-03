Investigate a symbol in an indexed codebase - find callers, callees, dead code status, and dependency cycles using rgr.

Usage: /investigate-symbol <repo> <SymbolName>

Steps:
1. Run `rgr graph callers $ARGUMENTS --edge-types CALLS,INSTANTIATES,IMPLEMENTS --json` to find who references this symbol
2. Run `rgr graph callees $ARGUMENTS --depth 1 --json` to find what this symbol depends on
3. Run `rgr graph dead $ARGUMENTS --kind SYMBOL --json` and check if the symbol appears in results
4. Run `rgr graph cycles $ARGUMENTS --json` and check if the symbol's module has circular dependencies
5. Summarize: what the symbol is, who depends on it, what it depends on, whether it's dead code, whether its module has cycles
