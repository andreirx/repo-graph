<!-- requirements-assurance-implementation-v1
{
  "formatVersion": 1,
  "kind": "implementation-allocation",
  "workItemId": "DEPS-ECOSYSTEM-PARTITION-1",
  "baselinePath": "docs/requirements/baselines/DEPS-ECOSYSTEM-PARTITION-1-INPUT-1.json",
  "parentRequirementIds": [
    "RG-REQ-002",
    "RG-REQ-006",
    "RG-REQ-011",
    "RG-REQ-012"
  ],
  "implements": [
    "RG-REQ-006-L08",
    "RG-REQ-006-L09",
    "RG-REQ-006-L10",
    "RG-REQ-002-L06"
  ],
  "preserves": [
    "RG-REQ-006-L06",
    "RG-REQ-006-L07",
    "RG-REQ-002-L08",
    "RG-REQ-011-L06"
  ],
  "preservationObligationIds": [
    "P-DEP-01",
    "P-DEP-02",
    "P-DEP-03",
    "P-DEP-04",
    "P-DEP-05"
  ],
  "changes": [
    "RG-REQ-012-L06"
  ],
  "acceptanceBoundary": "The `deps list` human and JSON outputs of the candidate rmap against the before binary (both built at the recorded base revision / the candidate) on FOUR pre-provisioned isolated roots — django (fresh index), nginx (fresh index), gstreamer and FRAKTAG (copies of the retained audit-v0.18.0 stores) — plus cargo test -p repo-graph-module-queries (whole crate), -p repo-graph-storage --lib module_edges_support, -p repo-graph-daemon-runtime --lib (WHOLE unit suite) and --test deps_attrib_nested_workspace, -p repo-graph-rgr --lib presentation::deps_list, -p repo-graph-classification --lib dep_reduce, as named per check.",
  "candidatePaths": [
    "rust/crates/module-queries/src/deps/compose.rs",
    "rust/crates/storage/src/crud/module_edges_support.rs",
    "rust/crates/daemon-runtime/src/deps_headline.rs",
    "rust/crates/daemon-runtime/src/reader_context.rs",
    "rust/crates/daemon-runtime/src/dispatch.rs",
    "rust/crates/rgr/src/presentation/deps_list.rs"
  ],
  "postReviewRecordPaths": [
    "docs/assurance/DEPS-ECOSYSTEM-PARTITION-1/verification.json",
    "docs/assurance/DEPS-ECOSYSTEM-PARTITION-1/implementation-review.json"
  ],
  "candidateExclusions": [
    {
      "pathPrefix": ".agent-manager/",
      "reason": "local relay state and raw run/progress evidence"
    },
    {
      "pathPrefix": "rust/target/",
      "reason": "reproducible Cargo build output"
    }
  ],
  "checks": [
    {
      "checkId": "DEP-C01",
      "obligationIds": [
        "RG-REQ-006-L08",
        "RG-REQ-002-L06",
        "P-DEP-02"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-module-queries --lib deps 2>&1 | tee /tmp/dep-c01.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/dep-c01.txt && for t in observed_reference_from_another_ecosystem_file_is_skipped_and_counted observed_reference_from_same_ecosystem_file_is_admitted none_detected_view_admits_every_observed_reference_and_counts_none path_ecosystem_maps_extension_to_manifest_ecosystem; do grep -qE \"^test .*$t .* ok$\" /tmp/dep-c01.txt || { echo \"MISSING $t\"; exit 1; }; done",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "module-queries compose tests: three NEW tests in rust/crates/module-queries/src/deps/compose.rs (a `.py`-sourced external reference under ecosystem `npm` is skipped and counted in `cross_ecosystem` — it never reaches the module bucket or `admit_observed`; a `.ts`-sourced reference under `npm` is admitted exactly as today; under the `none-detected` view (`ecosystem_prefix` None) every reference is admitted and `cross_ecosystem == 0`) plus the existing `path_ecosystem_maps_extension_to_manifest_ecosystem`"
      },
      "expected": "exit 0: the observed side is gated by the ONE existing `path_ecosystem` predicate (compose.rs:615-624) — the same predicate the declared side uses at compose.rs:244 — and the skipped references are counted, never dropped silently; the none-detected view is exempt (P-DEP-02); the predicate's own test remains green"
    },
    {
      "checkId": "DEP-C02",
      "obligationIds": [
        "RG-REQ-006-L08",
        "RG-REQ-011-L06"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-storage --lib module_edges_support 2>&1 | tee /tmp/dep-c02.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/dep-c02.txt && grep -qE '^test .*external_import_facts_carry_the_source_file_path .* ok$' /tmp/dep-c02.txt",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "NEW storage test `external_import_facts_carry_the_source_file_path` in rust/crates/storage/src/crud/module_edges_support.rs tests: `get_external_imports_for_snapshot` (:285-320) now joins `files` and returns `ExternalImportFact.source_file_path` (the repo-relative path) for an `external_library_candidate` row, with the ORDER BY unchanged (`source_file_uid ASC, specifier ASC`)"
      },
      "expected": "exit 0: the fact carries the source path read from the store (the template is the existing `get_external_imports_with_locations` join at :440-470); no schema change, no new persisted column"
    },
    {
      "checkId": "DEP-C03",
      "obligationIds": [
        "RG-REQ-006-L09",
        "RG-REQ-006-L10",
        "RG-REQ-002-L06",
        "RG-REQ-002-L08",
        "P-DEP-03"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-daemon-runtime --lib reader_context 2>&1 | tee /tmp/dep-c03.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/dep-c03.txt && for t in reader_note_names_only_languages_without_a_reader reader_note_lists_parsed_manifests_of_other_ecosystems_with_counts_and_flag reader_note_without_parsed_manifests_is_byte_identical_to_before; do grep -qE \"^test .*$t .* ok$\" /tmp/dep-c03.txt || { echo \"MISSING $t\"; exit 1; }; done && cargo test -p repo-graph-daemon-runtime --lib deps_headline 2>&1 | tee /tmp/dep-c03b.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/dep-c03b.txt && for t in unattributed_names_cross_ecosystem_references_and_the_flag unattributed_appends_the_manifest_scope_remainder_after_the_cross_ecosystem_clause unattributed_without_cross_ecosystem_references_is_byte_identical_to_before response_emits_basis_and_caveat_and_maven_limit; do grep -qE \"^test .*$t .* ok$\" /tmp/dep-c03b.txt || { echo \"MISSING $t\"; exit 1; }; done",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "NEW unit tests: reader_context.rs — `deps_reader_context_note` filters its language list by `language_deps_ecosystem(tok).is_none()` (gstreamer's tokens c/cpp/python/rust/java/javascript → `C/C++`) and appends the parsed-manifest inventory clause built from `ProvenanceRead::Tracked` records with `error == None`, grouped by the record's `ecosystem` token and rendered as `; N manifests of other ecosystems were parsed (4 cargo, 8 java, 1 npm) — see `deps list --ecosystem java`` (counts sorted by ecosystem token; the flag names the ecosystem with the most parsed manifests, ties alphabetical); with no parsed records of other ecosystems the sentence is byte-identical to today's. deps_headline.rs — `compute_unattributed` renders §2.2's cross-ecosystem sentence from `ComposeDependenciesResult.cross_ecosystem` + its per-ecosystem breakdown, appends the manifest-scope remainder clause when it is non-zero, and is byte-identical to today's output when `cross_ecosystem == 0`; the existing `response_emits_basis_and_caveat_and_maven_limit` stays green"
      },
      "expected": "exit 0: the reader-absence claim names only reader-less languages and lists the manifests that were parsed with the flag that renders them (006-L09/L10, derived from snapshot facts — 002-L06; no basis code or internal token in the sentence — 002-L08); the ≥10% materiality gate (`MATERIAL_LANGUAGE_SHARE_NUM`, reader_context.rs:175) and `secondary_material_ecosystems` remain unchanged (P-DEP-03); repos without cross-ecosystem references or foreign parsed manifests render byte-identically (P-DEP-01)"
    },
    {
      "checkId": "DEP-C04",
      "obligationIds": [
        "RG-REQ-006-L06",
        "RG-REQ-006-L07",
        "RG-REQ-006-L10",
        "RG-REQ-012-L06",
        "P-DEP-05"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-rgr --lib presentation::deps_list 2>&1 | tee /tmp/dep-c04.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/dep-c04.txt && for t in asgiref_reads_used_with_computed_basis_tzdata_stays_unobserved maven_capability_limit_names_the_gap_and_suppresses_downgraded present_but_unparsed_manifest_renders_honest_no_record_never_assumed_no_source cross_ecosystem_field_defaults_to_zero_from_an_older_daemon; do grep -qE \"^test .*$t .* ok$\" /tmp/dep-c04.txt || { echo \"MISSING $t\"; exit 1; }; done && cargo test -p repo-graph-classification --lib dep_reduce 2>&1 | tee /tmp/dep-c04b.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/dep-c04b.txt",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "rgr deps_list tests: the three preserved tests named in RG-REQ-006-L07/L10 plus the NEW `cross_ecosystem_field_defaults_to_zero_from_an_older_daemon` (`DepsListResponse.cross_ecosystem: u64` with `#[serde(default)]`, deps_list.rs:86-176 — an envelope without the key deserializes to 0 and renders as today); classification dep_reduce tests (006-L06 head reduction untouched)"
      },
      "expected": "exit 0: per-row basis, Maven capability limit, provenance rendering and head reduction remain unchanged (no behavior change — P-DEP-05); the JSON field is additive and an older daemon's envelope still renders (012-L06, reported)"
    },
    {
      "checkId": "DEP-C05",
      "obligationIds": [
        "P-DEP-05",
        "RG-REQ-006-L07"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "cargo test -p repo-graph-daemon-runtime --lib 2>&1 | tee /tmp/dep-c05.txt | grep -E '^test result: ok\\. [0-9]+ passed; 0 failed' && ! grep -E '^test .* FAILED$' /tmp/dep-c05.txt && cargo test -p repo-graph-module-queries 2>&1 | tee /tmp/dep-c05b.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/dep-c05b.txt && cargo test -p repo-graph-daemon-runtime --test deps_attrib_nested_workspace 2>&1 | tee /tmp/dep-c05c.txt | grep -E '^test result: ok\\.' && ! grep -E 'FAILED|panicked' /tmp/dep-c05c.txt",
        "cwd": "rust",
        "environment": "candidate tree",
        "inputs": "the WHOLE daemon-runtime unit suite (the acceptance boundary of every touched daemon crate — lesson of EXPLAIN-CYCLES-HONEST-1), the whole module-queries crate, and the end-to-end deps JSON test rust/crates/daemon-runtime/tests/deps_attrib_nested_workspace.rs (`nested_workspace_npm_attributes_and_default_view_states_java_truth`, which asserts `ecosystem`/`other_ecosystems`) — run to completion in the foreground"
      },
      "expected": "exit 0: no behavior change anywhere else in the daemon or the deps composition — every existing deps test (compose attach tests, deps_headline payload tests, deps_ecosystem_presence, the nested-workspace e2e) remains green"
    },
    {
      "checkId": "DEP-C06",
      "obligationIds": [
        "RG-REQ-006-L08",
        "RG-REQ-002-L06",
        "RG-REQ-012-L06"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/django' && export RMAP_STATE_ROOT=/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-django RMAP_SOCKET_PATH=/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-django/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-before:$PATH\" \"/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-before/rmap\" deps list --ecosystem npm > /tmp/dep-django-npm-before.txt 2> /tmp/dep-django-npm-before.txt.err ) || { echo \"RUN-FAILED /tmp/dep-django-npm-before.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/django' && export RMAP_STATE_ROOT=/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-django RMAP_SOCKET_PATH=/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-django/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" deps list --ecosystem npm > /tmp/dep-django-npm-after.txt 2> /tmp/dep-django-npm-after.txt.err ) || { echo \"RUN-FAILED /tmp/dep-django-npm-after.txt\"; exit 1; } && grep -qE '^  used 0 · no static import found 6 · undeclared 102 · builtins 10' /tmp/dep-django-npm-before.txt && grep -qE '^  used 0 · no static import found 6 · undeclared 0( · |$)' /tmp/dep-django-npm-after.txt && ! grep -q 'undeclared: abc, argparse' /tmp/dep-django-npm-after.txt && grep -E '^⚠ [0-9,]+ of [0-9,]+ external references are imports from files outside the npm ecosystem \\([0-9,]+ python\\) — see `deps list --ecosystem python`' /tmp/dep-django-npm-after.txt && python3 -c \"import re;s=open('/tmp/dep-django-npm-after.txt').read();m=re.search(r'^⚠ ([0-9,]+) of ([0-9,]+) external',s,re.M);a,b=[int(x.replace(',','')) for x in m.groups()];assert a*2>b and b==13967,(a,b);print('cross',a,'of',b)\"",
        "cwd": ".",
        "environment": "pre-provisioned isolated root /private/tmp/DEPS-ECOSYSTEM-PARTITION-1-django (fresh index of /Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/django built by the base-revision binary on 2026-09-19; registry filtered to django), served with the before binary (/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-before/rmap{,d} copied from rust/target/release at the recorded base revision BEFORE any candidate edit) and the candidate binary (rust/target/release after `cargo build --release --bin rmap --bin rmapd`), both under RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off",
        "inputs": "django `deps list --ecosystem npm` on the SAME index before/after: before renders `used 0 · no static import found 6 · undeclared 102 · builtins 10` with `undeclared: abc, argparse, asgiref.local` (RC-6, read from the product on 2026-09-19); after, the Python-file references are outside the npm view, named in the ⚠ line with the flag to see them; the numbers (13,967 total; the Python-file share) are REPORTED from the output — the assertion is structural (majority of the references are Python-file imports; the total is the index's 13967)"
      },
      "expected": "exit 0: `undeclared 0` and no `undeclared: abc, argparse` under the npm view; the ⚠ line names the cross-ecosystem count and the `--ecosystem python` flag; `used 0 · no static import found 6` unchanged; `builtins` REPORTED (not predicted)"
    },
    {
      "checkId": "DEP-C07",
      "obligationIds": [
        "P-DEP-01",
        "RG-REQ-006-L06",
        "RG-REQ-006-L07"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/django' && export RMAP_STATE_ROOT=/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-django RMAP_SOCKET_PATH=/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-django/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-before:$PATH\" \"/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-before/rmap\" deps list > /tmp/dep-django-default-before.txt 2> /tmp/dep-django-default-before.txt.err ) || { echo \"RUN-FAILED /tmp/dep-django-default-before.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/django' && export RMAP_STATE_ROOT=/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-django RMAP_SOCKET_PATH=/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-django/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" deps list > /tmp/dep-django-default-after.txt 2> /tmp/dep-django-default-after.txt.err ) || { echo \"RUN-FAILED /tmp/dep-django-default-after.txt\"; exit 1; } && diff <(grep -v '^⚠ ' /tmp/dep-django-default-before.txt) <(grep -v '^⚠ ' /tmp/dep-django-default-after.txt) && grep -q 'asgiref (51 import sites, 131 call sites)' /tmp/dep-django-default-after.txt && grep -E '^⚠ ' /tmp/dep-django-default-before.txt && grep -E '^⚠ ' /tmp/dep-django-default-after.txt",
        "cwd": ".",
        "environment": "same roots/binaries as DEP-C06",
        "inputs": "django default view (auto-detected python) on the same index: every line except the ⚠ headline byte-identical before/after (the module row `.  [pyproject.toml]`, `used 2 · no static import found 1 · undeclared 0 · self 1 · builtins 128`, the asgiref/sqlparse basis); the ⚠ line before is `419 of 13967 external references not attributed to a declared manifest (imported files outside a parsed manifest scope)`; after, any non-Python-file references among those 419 are named as cross-ecosystem and the remainder keeps the manifest-scope clause — both lines are printed for the report (REPORTED, never predicted)"
      },
      "expected": "exit 0: rows, counts and bases unchanged on the dominant-ecosystem view (no behavior change on the observed references that belong to the view); only the ⚠ headline may change, and it is reported verbatim"
    },
    {
      "checkId": "DEP-C08",
      "obligationIds": [
        "RG-REQ-012-L06",
        "RG-REQ-006-L08"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/django' && export RMAP_STATE_ROOT=/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-django RMAP_SOCKET_PATH=/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-django/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" deps list --ecosystem npm --json > /tmp/dep-django-npm-after.json 2> /tmp/dep-django-npm-after.json.err ) || { echo \"RUN-FAILED /tmp/dep-django-npm-after.json\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/django' && export RMAP_STATE_ROOT=/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-django RMAP_SOCKET_PATH=/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-django/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" deps list --json > /tmp/dep-django-default-after.json 2> /tmp/dep-django-default-after.json.err ) || { echo \"RUN-FAILED /tmp/dep-django-default-after.json\"; exit 1; } && python3 -c \"import json,re\nd=json.load(open('/tmp/dep-django-npm-after.json'));s=open('/tmp/dep-django-npm-after.txt').read();m=re.search(r'^⚠ ([0-9,]+) of ',s,re.M);n=int(m.group(1).replace(',',''));assert d['cross_ecosystem']==n,(d['cross_ecosystem'],n);assert d['total_external_imports']==13967;e=json.load(open('/tmp/dep-django-default-after.json'));assert isinstance(e['cross_ecosystem'],int);print('npm cross',n,'default cross',e['cross_ecosystem'])\"",
        "cwd": ".",
        "environment": "same root as DEP-C06, candidate binary",
        "inputs": "the deps JSON envelope (`build_deps_list_response`, deps_headline.rs:441-469) gains the additive top-level `cross_ecosystem` (integer) beside `total_external_imports`; the human ⚠ line states the same number"
      },
      "expected": "exit 0: the fact visible in the human line is present in JSON and equal (012-L06 — the human/JSON asymmetry is none for this fact); the field is additive"
    },
    {
      "checkId": "DEP-C09",
      "obligationIds": [
        "RG-REQ-006-L09",
        "RG-REQ-006-L10",
        "RG-REQ-002-L06",
        "P-DEP-02"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/gstreamer' && export RMAP_STATE_ROOT=/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-gst RMAP_SOCKET_PATH=/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-gst/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-before:$PATH\" \"/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-before/rmap\" deps list > /tmp/dep-gst-default-before.txt 2> /tmp/dep-gst-default-before.txt.err ) || { echo \"RUN-FAILED /tmp/dep-gst-default-before.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/gstreamer' && export RMAP_STATE_ROOT=/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-gst RMAP_SOCKET_PATH=/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-gst/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" deps list > /tmp/dep-gst-default-after.txt 2> /tmp/dep-gst-default-after.txt.err ) || { echo \"RUN-FAILED /tmp/dep-gst-default-after.txt\"; exit 1; } && grep -qE '^⚠ no dependency-manifest reader for C/C\\+\\+/Java/JavaScript/Python/Rust on this build; 14216 external includes observed, not attributed to packages$' /tmp/dep-gst-default-before.txt && grep -qE '^⚠ no dependency-manifest reader for C/C\\+\\+ on this build; 14216 external includes observed, not attributed to packages; 13 manifests of other ecosystems were parsed \\(4 cargo, 8 java, 1 npm\\) — see `deps list --ecosystem java`$' /tmp/dep-gst-default-after.txt && diff <(grep -v '^⚠ ' /tmp/dep-gst-default-before.txt) <(grep -v '^⚠ ' /tmp/dep-gst-default-after.txt) && grep -c 'unknown-external' /tmp/dep-gst-default-after.txt | grep -qx 7",
        "cwd": ".",
        "environment": "pre-provisioned isolated root /private/tmp/DEPS-ECOSYSTEM-PARTITION-1-gst: a COPY of the retained audit-v0.18.0 gstreamer store (~/repo-graph-retained/audit-v0.18.0/databases/4c3ef543f43413bc.db, 2.4 GB; registry filtered to that entry) — never the retained root itself, never a reindex; before/candidate binaries as in DEP-C06",
        "inputs": "gstreamer default view on the same store: before, the reader sentence names `C/C++/Java/JavaScript/Python/Rust` (RC-7; read from the product on 2026-09-19) although the same snapshot's `extraction_diagnostics_json.deps_manifests` holds 13 parsed records — cargo 4, java 8, npm 1 (verified on the store copy with sqlite on 2026-09-19; `deps_manifests_present` = {cargo 4, npm 1, python 0}, no java key, so the sentence is built from the parsed records, not the present map); after, the sentence names `C/C++` only and lists the parsed manifests with the flag; the 7 `none-detected` rows (ci, scripts, subprojects, …) with their `unknown-external` counts are byte-identical"
      },
      "expected": "exit 0: the reader-absence claim names only reader-less languages and the parsed manifests of other ecosystems with counts and the flag that renders them (the ecosystem token the flag accepts — `java`, not the manifest kind `gradle`); the none-detected rows keep every import (P-DEP-02)"
    },
    {
      "checkId": "DEP-C10",
      "obligationIds": [
        "P-DEP-02",
        "RG-REQ-006-L10"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/gstreamer' && export RMAP_STATE_ROOT=/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-gst RMAP_SOCKET_PATH=/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-gst/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-before:$PATH\" \"/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-before/rmap\" deps list --ecosystem cargo > /tmp/dep-gst-cargo-before.txt 2> /tmp/dep-gst-cargo-before.txt.err ) || { echo \"RUN-FAILED /tmp/dep-gst-cargo-before.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/gstreamer' && export RMAP_STATE_ROOT=/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-gst RMAP_SOCKET_PATH=/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-gst/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" deps list --ecosystem cargo > /tmp/dep-gst-cargo-after.txt 2> /tmp/dep-gst-cargo-after.txt.err ) || { echo \"RUN-FAILED /tmp/dep-gst-cargo-after.txt\"; exit 1; } && diff <(grep -v '^⚠ ' /tmp/dep-gst-cargo-before.txt) <(grep -v '^⚠ ' /tmp/dep-gst-cargo-after.txt) && grep -c 'Cargo.toml\\]' /tmp/dep-gst-cargo-after.txt | grep -qx 4 && grep -E '^⚠ ' /tmp/dep-gst-cargo-before.txt && grep -E '^⚠ ' /tmp/dep-gst-cargo-after.txt",
        "cwd": ".",
        "environment": "same root/binaries as DEP-C09",
        "inputs": "gstreamer `--ecosystem cargo` on the same store: the 4 Cargo module rows (`[<path>/Cargo.toml]`, `used 13 · no static import found 5 …` etc.) byte-identical; the ⚠ line before is `13984 of 14216 external references not attributed to a declared manifest (…)`; after, the references from files outside the cargo ecosystem (C/C++ sources have no manifest ecosystem — `path_ecosystem` None) are named per §2.2 with `without a manifest ecosystem` as their group; both lines printed for the report"
      },
      "expected": "exit 0: the cargo view's rows and counts are unchanged; the headline reports the split honestly (REPORTED)"
    },
    {
      "checkId": "DEP-C11",
      "obligationIds": [
        "P-DEP-01",
        "RG-REQ-006-L09"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/nginx' && export RMAP_STATE_ROOT=/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-nginx RMAP_SOCKET_PATH=/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-nginx/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-before:$PATH\" \"/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-before/rmap\" deps list > /tmp/dep-nginx-before.txt 2> /tmp/dep-nginx-before.txt.err ) || { echo \"RUN-FAILED /tmp/dep-nginx-before.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/nginx' && export RMAP_STATE_ROOT=/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-nginx RMAP_SOCKET_PATH=/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-nginx/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" deps list > /tmp/dep-nginx-after.txt 2> /tmp/dep-nginx-after.txt.err ) || { echo \"RUN-FAILED /tmp/dep-nginx-after.txt\"; exit 1; } && diff /tmp/dep-nginx-before.txt /tmp/dep-nginx-after.txt && grep -qE '^⚠ no dependency-manifest reader for C/C\\+\\+ on this build; 56 external includes observed, not attributed to packages$' /tmp/dep-nginx-after.txt",
        "cwd": ".",
        "environment": "pre-provisioned isolated root /private/tmp/DEPS-ECOSYSTEM-PARTITION-1-nginx (fresh index of /Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/nginx by the base-revision binary; 4 s); before/candidate binaries as in DEP-C06",
        "inputs": "nginx default view (C only; `deps_manifests` empty): the whole output byte-identical before/after, the reader sentence unchanged (read from the product on 2026-09-19)"
      },
      "expected": "exit 0: a repo with no other-ecosystem manifests and no cross-ecosystem references renders byte-identically — no behavior change (P-DEP-01)"
    },
    {
      "checkId": "DEP-C12",
      "obligationIds": [
        "P-DEP-01",
        "RG-REQ-006-L08"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "( cd '/Users/apple/Documents/APLICATII BIJUTERIE/FRAKTAG' && export RMAP_STATE_ROOT=/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-fraktag RMAP_SOCKET_PATH=/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-fraktag/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-before:$PATH\" \"/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-before/rmap\" deps list > /tmp/dep-fraktag-before.txt 2> /tmp/dep-fraktag-before.txt.err ) || { echo \"RUN-FAILED /tmp/dep-fraktag-before.txt\"; exit 1; } && ( cd '/Users/apple/Documents/APLICATII BIJUTERIE/FRAKTAG' && export RMAP_STATE_ROOT=/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-fraktag RMAP_SOCKET_PATH=/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-fraktag/d.sock RMAP_TRANSPORT=stdio RMAP_AUTO_ENRICH=off RMAP_AUTO_RETENTION=off && PATH=\"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release:$PATH\" \"/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/rust/target/release/rmap\" deps list > /tmp/dep-fraktag-after.txt 2> /tmp/dep-fraktag-after.txt.err ) || { echo \"RUN-FAILED /tmp/dep-fraktag-after.txt\"; exit 1; } && diff <(grep -v '^⚠ ' /tmp/dep-fraktag-before.txt) <(grep -v '^⚠ ' /tmp/dep-fraktag-after.txt) && grep -qE '^⚠ 30 of 733 external references not attributed' /tmp/dep-fraktag-before.txt && grep -E '^⚠ 30 of 733 external references are imports from files outside the npm ecosystem \\(' /tmp/dep-fraktag-after.txt",
        "cwd": ".",
        "environment": "pre-provisioned isolated root /private/tmp/DEPS-ECOSYSTEM-PARTITION-1-fraktag: a COPY of the retained audit-v0.18.0 FRAKTAG store (databases/e444686289ee2bf9.db, 14 MB; registry filtered); before/candidate binaries as in DEP-C06",
        "inputs": "FRAKTAG default (npm) view on the same store: rows byte-identical; the count `30 of 733` preserved — RC-6's store reading says those 30 are Python-file references; the breakdown and flag the product prints are REPORTED (not asserted) — only the count and the cross-ecosystem form are asserted"
      },
      "expected": "exit 0: the count `30 of 733` is stable and the rows are unchanged (P-DEP-01); the headline now names what the 30 are (006-L08) instead of folding them into the manifest-scope clause — the breakdown is reported verbatim"
    },
    {
      "checkId": "DEP-C13",
      "obligationIds": [
        "RG-REQ-011-L06",
        "P-DEP-04"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "test \"$(shasum -a 256 \"$HOME/Library/Application Support/repo-graph/registry.json\" | cut -c1-16)\" = \"$(grep -E '^registry-before ' .agent-manager/slices/DEPS-ECOSYSTEM-PARTITION-1/build-progress.md | tail -n1 | awk '{print $2}')\" && PIDS=$(pgrep -x rmapd | paste -sd, -) && { test -z \"$PIDS\" || test \"$(ps -o command= -p \"$PIDS\" | grep -vc '/\\.local/bin/rmapd')\" = 0; } && ls -d /private/tmp/DEPS-ECOSYSTEM-PARTITION-1-gst /private/tmp/DEPS-ECOSYSTEM-PARTITION-1-django /private/tmp/DEPS-ECOSYSTEM-PARTITION-1-nginx /private/tmp/DEPS-ECOSYSTEM-PARTITION-1-fraktag >/dev/null && ! ls -d /private/tmp/DEPS-ECOSYSTEM-PARTITION-1-before 2>/dev/null && git worktree list | grep -vc 'm-r2-baseline-worktree\\|\\[main\\]' | grep -qx 0",
        "cwd": ".",
        "environment": "the operator's real registry (read-only, digest only); the four pre-provisioned roots stay for the manager's closeout sweep; the before-binary dir is removed by the builder after the last proof",
        "inputs": "build-progress.md records `registry-before <16-hex>` BEFORE the first proof (the value printed by `shasum -a 256 \"$HOME/Library/Application Support/repo-graph/registry.json\" | cut -c1-16`) and the same command's value after cleanup; every rmap ran under RMAP_STATE_ROOT=/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-* with stdio and the auto passes off; no daemon left from either binary (checked by exact process name `rmapd` and its command path — the operator's launchd daemon at ~/.local/bin/rmapd is the only one allowed); no extra worktree"
      },
      "expected": "exit 0: no behavior change outside the isolated roots — the operator's registry digest remains identical before and after, no candidate/before daemon survives, the four proof roots are intact for the manager, the before dir is gone, no extra worktree"
    },
    {
      "checkId": "DEP-C14",
      "obligationIds": [
        "RG-REQ-006-L08",
        "RG-REQ-002-L08"
      ],
      "owner": "builder",
      "method": {
        "kind": "command",
        "command": "git diff --check && (cd rust && cargo fmt --check -p repo-graph-module-queries -p repo-graph-storage -p repo-graph-daemon-runtime -p repo-graph-rgr && cargo clippy -p repo-graph-module-queries -p repo-graph-storage -p repo-graph-daemon-runtime -p repo-graph-rgr --tests -- -D warnings > /tmp/dep-clippy.log 2>&1 && tail -n1 /tmp/dep-clippy.log) && diff <(git status --short -- rust | sort) <(printf ' M rust/crates/daemon-runtime/src/deps_headline.rs\\n M rust/crates/daemon-runtime/src/dispatch.rs\\n M rust/crates/daemon-runtime/src/reader_context.rs\\n M rust/crates/module-queries/src/deps/compose.rs\\n M rust/crates/rgr/src/presentation/deps_list.rs\\n M rust/crates/storage/src/crud/module_edges_support.rs\\n' | sort) && ! grep -rn 'basis_code\\|SpecifierMatches\\|external_library_candidate' rust/crates/daemon-runtime/src/reader_context.rs rust/crates/daemon-runtime/src/deps_headline.rs | grep -v '^.*//' | grep -q 'format!'",
        "cwd": ".",
        "environment": "candidate tree",
        "inputs": "git status/diff of the candidate; rustfmt + clippy -D warnings over the four touched crates; the exact set of six modified paths and nothing else; no internal basis token formatted into a rendered sentence"
      },
      "expected": "exit 0: whitespace-, rustfmt- and clippy-clean (clippy's own exit status required); the working tree under rust/ holds exactly the six candidate paths; the new sentences carry no internal-diagnostic label (002-L08)"
    }
  ]
}
-->

# DEPS-ECOSYSTEM-PARTITION-1 — `deps list` partitions what it observed by ecosystem and names the readers it has

Status: SPECIFIED 2026-09-12; GROUNDED and re-verified on HEAD 7e20f60 on 2026-09-19 (every literal below read from the product on isolated roots that day) · Track: audit round six, Q5 (HIGH; RC-6 never worked / visibility regressed via DEPS-CLASSIFIER-1B; RC-7 never worked). CODE slice. Builder: claude / claude-opus-4-8; reviewer: codex / gpt-5.6-terra (human directive 2026-09-13).

## 0. Requirements allocation (the machine-readable block above is binding; this section explains it)

**Implements:** RG-REQ-006-L08 (the observed side is partitioned by the importing file's ecosystem under `--ecosystem`, and the skipped references are named — DEP-C01/C06/C08/C12), RG-REQ-006-L09 (the reader-absence sentence names only reader-less languages and lists the parsed manifests of other ecosystems with the flag — DEP-C03/C09), RG-REQ-006-L10 (every materially present ecosystem is named: the gstreamer failure recorded under L10 IS L09; Maven/provenance rendering preserved — DEP-C04/C09/C10), RG-REQ-002-L06 (coverage stated where it renders, derived from snapshot facts: the language tokens and the `deps_manifests` records — DEP-C03/C09).

**Changes (pre-authorised, REPORTED never predicted):** RG-REQ-012-L06 — the deps JSON envelope gains the additive top-level `cross_ecosystem` integer; the human ⚠ line states the same number (DEP-C08).

**Preserves:** RG-REQ-006-L06 / L07 (head reduction and per-row basis — DEP-C04/C07), RG-REQ-002-L08 (no internal label in the new sentences — DEP-C03/C14), RG-REQ-011-L06 (isolation — DEP-C13).

**Preservation obligations:**

| Id | Obligation | Proof |
|---|---|---|
| P-DEP-01 | Default views whose observed references all belong to the dominant ecosystem, and repos without other-ecosystem manifests, render byte-identically (nginx whole output; django, FRAKTAG rows and counts; the ⚠ headline is the only line that may change, and it is reported) | DEP-C07; DEP-C11; DEP-C12 |
| P-DEP-02 | The `none-detected` view is exempt from the partition: every observed reference is admitted, `cross_ecosystem == 0`, gstreamer's 7 rows keep their `unknown-external` counts; the `--ecosystem cargo` rows on gstreamer are unchanged | DEP-C01; DEP-C09; DEP-C10 |
| P-DEP-03 | The ≥10%-of-source materiality gate (`MATERIAL_LANGUAGE_SHARE_NUM`) and `secondary_material_ecosystems` are untouched; the §2.4 secondary-ecosystem line renders exactly as today | DEP-C03; DEP-C05 |
| P-DEP-04 | Every proof isolated; the operator's registry digest unchanged; no daemon or before-dir left; the four proof roots kept for the manager | DEP-C13 |
| P-DEP-05 | Every existing deps test — compose attach tests, deps_headline payload tests, deps_ecosystem_presence, the nested-workspace e2e, the rgr deps_list tests, dep_reduce — remains green; the whole daemon-runtime unit suite passes | DEP-C04; DEP-C05 |

## 1. Problem (ROOT-CAUSED — RC-6, RC-7; docs/audits/2026-09-08-root-causes-v0.18.0.md; re-verified on HEAD 7e20f60, 2026-09-19 — no slice file changed since the audit)

**RC-6.** `rust/crates/module-queries/src/deps/compose.rs:181-233` groups EVERY external reference (`unresolved_edges ⋈ nodes`, classification `external_library_candidate`, read by `storage/src/crud/module_edges_support.rs:285-320 get_external_imports_for_snapshot`) into the module bucket; the only gates inside the loop are module membership (:182), the key bias `insert_module_key_preferring_ecosystem` (:187-192) and `admit_observed` (:207-213, :652-658 — takes no file path). Only the DECLARED side is partitioned: `compose.rs:244` `if path_ecosystem(&dep.file_path) != Some(input.ecosystem.as_str()) { continue }` (the audit cited :240; the line moved by 4). The module-level prefix gate (:315-320) drops whole modules, never references. `ExternalImportFact` (:483-497) carries `source_file_uid`, `specifier`, `is_import_edge` — no path. Outward, on the isolated django index (2026-09-19): `deps list --ecosystem npm` → `used 0 · no static import found 6 · undeclared 102 · builtins 10` with `undeclared: abc, argparse, asgiref.local` — Python-file imports reconciled against package.json's 6 devDeps. Regression-or-never: the observed filter never existed (f634618, 2026-05-11); DEPS-LIST-REWRITE-1 (4055d27) added the declared-side gate; DEPS-CLASSIFIER-1B (d333214) made the npm row render at all.

**RC-7.** `rust/crates/daemon-runtime/src/reader_context.rs:134-148 deps_reader_context_note` joins `display_language_names(languages)` (:81-89) over EVERY language token of the snapshot without consulting `language_deps_ecosystem` (:102-110). Outward, on a copy of the retained gstreamer store (2026-09-19): `⚠ no dependency-manifest reader for C/C++/Java/JavaScript/Python/Rust on this build; 14216 external includes observed, not attributed to packages` — while the same snapshot's `extraction_diagnostics_json.deps_manifests` holds 13 parsed records: cargo 4, java 8, npm 1 (all `error: null`; `deps_manifests_present` = {cargo 4, npm 1, python 0}); `deps list --ecosystem cargo` already renders the 4 Cargo modules. The §2.4 secondary-ecosystem line is suppressed by the ≥10% gate (rust is 0.28% of gstreamer's code files) — that gate is correct and stays. Never worked (HONEST-DEGRADATION-IMPL-2 c8f1f1c; DEPS-LIST-REWRITE-1; DEPS-ATTRIB-2).

## 2. Contract

### 2.1 The fix, at the cause — one predicate reused, one sentence filtered, nothing new persisted

1. **Storage carries the source path (no schema change).** `get_external_imports_for_snapshot` joins `files f ON n.file_uid = f.file_uid` and selects `f.path AS source_file_path` (the template is `get_external_imports_with_locations`, module_edges_support.rs:440-470); `ExternalImportFact` gains `source_file_path: String`; ORDER BY stays `source_file_uid ASC, specifier ASC`.
2. **compose.rs partitions the observed side with the ONE existing predicate.** At the top of the `for import in &external_imports` loop (:181): `if ecosystem_prefix.is_some() && path_ecosystem(&import.source_file_path) != Some(input.ecosystem.as_str()) { cross.record(path_ecosystem(&import.source_file_path)); continue; }` — the same `path_ecosystem` (:615-624) the declared side uses at :244; `ecosystem_prefix` is the local derived at :150-156, so the `none-detected` view (`None`) admits everything. `ComposeDependenciesResult` (:41-47) gains `cross_ecosystem: usize` and `cross_ecosystem_by_source: Vec<(Option<&'static str>, usize)>` (or an equivalent small owned shape) — counts of skipped references grouped by the source file's ecosystem, `None` = a file with no manifest ecosystem (C/C++ sources, config).
3. **`compute_unattributed` names the split (deps_headline.rs:328-367).** When `cross_ecosystem > 0` the reason is `"{cross} of {total} external references are imports from files outside the {eco} ecosystem ({breakdown}){see}"` where `{breakdown}` lists the groups as `{count} {ecosystem-token}` sorted by count descending, ties by token, a `None` group rendered `{count} without a manifest ecosystem`; `{see}` is ` — see `deps list --ecosystem {tok}`` for the largest NAMED group, omitted when every skipped reference is without a manifest ecosystem. When the manifest-scope remainder (today's whole count) is still non-zero after the partition, the sentence continues `; {r} more not attributed to a declared manifest (imported files outside a parsed manifest scope)`. When `cross_ecosystem == 0` the output is byte-identical to today's (`"{n} of {total} external references not attributed to a declared manifest (imported files outside a parsed manifest scope)"` / `"all external references attributed or classified"`). `unattributed_external_imports` (the JSON count) = cross + remainder; the ⚠ line renders whenever it is non-zero, as today (deps_list.rs:260-265). Numbers render exactly as the existing headline renders them today — plain integers, no thousands separators (`419 of 13967` on django on 2026-09-19); the check regexes tolerate either form, the contract is: match the existing formatting, read from the product.
4. **`deps_reader_context_note` names only reader-less languages and lists the parsed manifests (reader_context.rs:134-148).** The language list is filtered by `language_deps_ecosystem(tok).is_none()` before `display_language_names`; the function takes the `ProvenanceRead` the dispatcher already holds (`dispatch.rs:6752-6753 read_manifest_provenance`) — `compute_unattributed`'s `none-detected` branch (:348-353) passes it through — and appends, when `Tracked(records)` holds records with `error == None` whose `ecosystem` is not the view's, `"; {n} manifests of other ecosystems were parsed ({counts}) — see `deps list --ecosystem {tok}`"` with `{counts}` as `{count} {token}` sorted by token (`4 cargo, 8 java, 1 npm`) and `{tok}` the token with the most parsed manifests, ties alphabetical (gstreamer → `java`). The token is the ecosystem token the `--ecosystem` flag accepts (`npm|cargo|python|java`), never the manifest kind (`gradle`, `pom.xml`). `Absent`/`Unavailable` provenance or no foreign parsed records ⇒ the sentence is byte-identical to today's. The ≥10% gate and `secondary_material_ecosystems` are not touched.
5. **JSON (deps_headline.rs:441-469) gains `cross_ecosystem`** beside `total_external_imports`; `DepsListResponse` (rgr deps_list.rs:86-176) gains `#[serde(default)] pub cross_ecosystem: u64`; the human render prints the daemon's reason verbatim as today — no rgr render logic changes. `dispatch.rs` changes only at the `compute_unattributed` call (:6894-6895) to pass the provenance already read. `deps_reader_context_note`'s SECOND caller — the none-detected JSON `reader_context` field at deps_headline.rs:564 (inside `build_deps_list_response`) — is updated in the same file to the enriched sentence, re-sourced from the already-computed `unattributed.reason` that `build_deps_list_response` already receives (never a fresh call needing provenance `build_deps_list_response` does not hold); both `deps_reader_context_note` callers thus stay in deps_headline.rs and dispatch.rs's only change remains the `compute_unattributed` call at :6894-6895.

### 2.2 Evidence taxonomy (one row = one bound test)

| Input | Outcome | Bound test / check |
|---|---|---|
| observed reference from a `.py` file under `--ecosystem npm` | skipped, counted under `python` | `observed_reference_from_another_ecosystem_file_is_skipped_and_counted` (DEP-C01) |
| observed reference from a `.ts` file under `--ecosystem npm` | admitted exactly as today | `observed_reference_from_same_ecosystem_file_is_admitted` (DEP-C01) |
| any reference under the `none-detected` view | admitted; `cross_ecosystem == 0` | `none_detected_view_admits_every_observed_reference_and_counts_none` (DEP-C01) |
| observed reference from a file with no manifest ecosystem (`.c`) under `--ecosystem cargo` | skipped, counted under `without a manifest ecosystem`, no `see` flag for that group | DEP-C10 (gstreamer); the compose test above covers the `None` group |
| store row for an external reference | fact carries `source_file_path` | `external_import_facts_carry_the_source_file_path` (DEP-C02) |
| `cross_ecosystem > 0`, remainder 0 | the cross-ecosystem sentence with breakdown + flag | `unattributed_names_cross_ecosystem_references_and_the_flag` (DEP-C03) |
| `cross_ecosystem > 0`, remainder > 0 | sentence + `; N more not attributed …` clause | `unattributed_appends_the_manifest_scope_remainder_after_the_cross_ecosystem_clause` (DEP-C03) |
| `cross_ecosystem == 0` | byte-identical to today | `unattributed_without_cross_ecosystem_references_is_byte_identical_to_before` (DEP-C03) |
| languages c/cpp/python/rust/java/javascript, 13 parsed foreign manifests | `C/C++` + `; 13 manifests of other ecosystems were parsed (4 cargo, 8 java, 1 npm) — see `deps list --ecosystem java`` | `reader_note_names_only_languages_without_a_reader`, `reader_note_lists_parsed_manifests_of_other_ecosystems_with_counts_and_flag` (DEP-C03); DEP-C09 |
| languages c only, no parsed manifests (`Absent`) | byte-identical to today | `reader_note_without_parsed_manifests_is_byte_identical_to_before` (DEP-C03); DEP-C11 |
| provenance `Unavailable { reason }` | the reader sentence renders as today (no inventory clause; no reason text in the sentence) | covered by the same reader_context test file — assert it explicitly in `reader_note_without_parsed_manifests_is_byte_identical_to_before` |
| JSON envelope from an older daemon (no `cross_ecosystem`) | deserializes to 0, renders as today | `cross_ecosystem_field_defaults_to_zero_from_an_older_daemon` (DEP-C04) |

### 2.3 Outward proof (what a user of the product gains)

django `deps list --ecosystem npm`: `used 0 · no static import found 6 · undeclared 0 …` and `⚠ 139xx of 13967 external references are imports from files outside the npm ecosystem (139xx python) — see `deps list --ecosystem python`` instead of 102 phantom undeclared npm packages named `abc, argparse, asgiref.local`. gstreamer `deps list`: `⚠ no dependency-manifest reader for C/C++ on this build; 14216 external includes observed, not attributed to packages; 13 manifests of other ecosystems were parsed (4 cargo, 8 java, 1 npm) — see `deps list --ecosystem java`` instead of naming Rust/Python/Java/JavaScript as reader-less; `--ecosystem cargo` renders the 4 Cargo modules as today. FRAKTAG's `30 of 733` are named as Python-file imports with the flag to see them. nginx unchanged.

## 3. Regression watch

| Preserved | What would regress | Proof |
|---|---|---|
| RG-REQ-006-L06/L07, P-DEP-05 | head reduction; per-row basis; asgiref row | DEP-C04 (rgr + dep_reduce); DEP-C07 (django rows byte-identical) |
| RG-REQ-006-L10 | Maven refusal; provenance family; the 4 Cargo rows | DEP-C04; DEP-C10 |
| P-DEP-01 | byte-stability of nginx, django rows, FRAKTAG rows/count | DEP-C07; DEP-C11; DEP-C12 |
| P-DEP-02 | none-detected rows lose imports; cargo view rows move | DEP-C01; DEP-C09; DEP-C10 |
| P-DEP-03 | the ≥10% gate or the secondary line | DEP-C03; DEP-C05 (deps_ecosystem_presence tests inside the whole suite) |
| RG-REQ-002-L08 | a basis code / internal token in the new sentences | DEP-C14 |
| RG-REQ-012-L06 | JSON/human disagree on the count | DEP-C08 |
| RG-REQ-011-L06, P-DEP-04 | isolation | DEP-C13 |

## 4. Stop conditions

Frozen: the ≥10% gate, manifest parser shapes, head reduction, storage schema (the path is read, not stored anew), exit codes, the per-row `[no manifest — imports unattributed]` rendering in the none-detected view, orient/trust surfaces. No new predicate for ecosystems (the kernel is `path_ecosystem`; `language_deps_ecosystem` for tokens). Nothing outside the six candidate paths (a test that must move to another file is a finding, not an edit). STANDING HONESTY RULES (no `unwrap_or(0)`-class defaults on the new counts; a failed provenance read stays `Unavailable`, never "no manifests"). Unmet DoD → STOP. Do NOT commit. Run every check to completion in the FOREGROUND; end your turn only with the evidence object.

## 5. Validation (ORDERED; `build-progress.md` after EACH step)

0. On the CLEAN tree: record `git rev-parse HEAD` in build-progress.md (the relay's recorded base revision; read `status.json` tolerantly if you compare — the field may be absent) → `(cd rust && cargo build --release --bin rmap --bin rmapd)` (a no-op on the warm cache) → `mkdir -p /private/tmp/DEPS-ECOSYSTEM-PARTITION-1-before && cp rust/target/release/rmap rust/target/release/rmapd /private/tmp/DEPS-ECOSYSTEM-PARTITION-1-before/` — the before binary. Record `registry-before <digest>` in build-progress.md. Verify the four pre-provisioned roots exist (`ls -d /private/tmp/DEPS-ECOSYSTEM-PARTITION-1-{gst,django,nginx,fraktag}`); if any is missing (macOS purges idle /private/tmp files), rebuild it per §7 and say so.
1. Failing tests FIRST (the §2.2 names), then the storage join, the compose partition, `compute_unattributed`, the reader note, the JSON field, the dispatch call.
2. Chunked gates DEP-C01 → C02 → C03 → C04, then DEP-C05 (whole daemon-runtime unit suite, foreground) — NEVER `cargo test --workspace`.
3. `(cd rust && cargo build --release --bin rmap --bin rmapd)` (candidate) → live proofs DEP-C06 → C07 → C08 → C09 → C10 → C11 → C12 on the pre-provisioned roots, both binaries on the SAME root (never two indexes compared byte-for-byte); every `/tmp/dep-*.txt` capture from THIS cycle's run.
4. DEP-C13 cleanup (remove the before dir, kill nothing that is not yours — `pgrep -f target/release/rmapd` must be empty after each stdio call anyway), DEP-C14 hygiene, hand-off with the evidence object (each check's outcome NESTED under `outcome`; `supportingEvidence` non-empty).

## 6. Definition of done

All fourteen checks pass; §2.3 holds on django, gstreamer, FRAKTAG; nginx byte-identical; the report carries the code-under-analysis examples (django `django/…py` stdlib-import lines that were counted as npm `undeclared` — quote two real sites with file:line, e.g. `from abc import ABCMeta, abstractmethod` at django/tasks/backends/base.py:1 and `import argparse` at django/core/management/base.py:6 (`abc`/`argparse` are the specifiers that surfaced as npm `undeclared`); gstreamer's 13 manifest paths from the store, e.g. `subprojects/gst-devtools/dots-viewer/package.json`; FRAKTAG's Python-file imports).

## 7. Corpus roots (pre-provisioned by the manager on 2026-09-19 with the base-revision binary; rebuild recipe)

- `/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-django`: fresh index of `/Users/apple/Documents/APLICATII BIJUTERIE/legacy-codebases/django` (`rmap index`, 19 s, stdio, auto passes off).
- `/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-nginx`: fresh index of `…/legacy-codebases/nginx` (4 s).
- `/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-gst`: `databases/4c3ef543f43413bc.db` copied from `~/repo-graph-retained/audit-v0.18.0/databases/` (2.4 GB; no writer held it — `lsof` empty, wal 0 bytes) + `registry.json` = the retained registry filtered to `repo_01m213efer0f7p1tzqj8pdg2dr` with `db_path` rewritten to the copy.
- `/private/tmp/DEPS-ECOSYSTEM-PARTITION-1-fraktag`: `databases/e444686289ee2bf9.db` (14 MB) copied the same way, entry `repo_01m213373wzs2p0hjwpb2b1fdx`.
- The retained root itself is NEVER served (a serving daemon writes into it — lesson 2026-09-07). The operator registry `~/Library/Application Support/repo-graph/registry.json` is read only for its digest.

## 8. Follow-ups (not this slice)

The per-row `[no manifest — imports unattributed]` rendering in the none-detected view (answered by the documented `--ecosystem` route; fixing it needs four composes). A `deps_manifests_present` java key (present counts for gradle/maven are not tracked at index time; the parsed records are). TS-ALIAS-RESOLUTION-1 and IMPORTS-WITNESS-UNION-1 (D-ECH-002) are separate.
