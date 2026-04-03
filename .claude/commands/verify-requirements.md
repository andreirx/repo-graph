Check whether declared requirements and verification obligations are satisfied.

Usage: /verify-requirements <repo>

Prerequisites: requirements must be declared with `rgr declare requirement` and obligations added with `rgr declare obligation`.

Steps:
1. Run `rgr declare list $ARGUMENTS --kind requirement --json` to list all requirements
2. Run `rgr graph obligations $ARGUMENTS --json` to evaluate all verification obligations
3. For each FAIL verdict, explain what evidence was checked and why it failed
4. For each MISSING_EVIDENCE verdict, explain what data needs to be imported
5. Produce a verification status report: total obligations, pass count, fail count, missing count
6. List specific remediation actions for each failed obligation
