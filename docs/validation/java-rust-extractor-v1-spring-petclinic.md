# Java Rust Extractor v1 Validation Report

**Repo:** spring-petclinic  
**Snapshot:** `spring-petclinic/2026-04-24T12:37:39.790Z/da633cee`  
**Validation Date:** 2026-04-24  
**Validator:** CLI-first validation (rmap commands only)

## 1. Validation Scope

Initial real-repo validation of Java extractor (`java-core:0.1.0`) integrated
into the Rust indexing pipeline. Target: verify extraction, queryability, and
trust report integration. NOT a production-readiness assessment.

## 2. CLI Commands Executed

### 2.1 Index

```bash
rmap index "/path/to/spring-petclinic" /tmp/petclinic.db
```

**Output:**
```
indexed 47 files, 359 nodes, 332 edges (1396 unresolved) → spring-petclinic/2026-04-24T12:37:39.790Z/da633cee
```

### 2.2 Trust

```bash
rmap trust /tmp/petclinic.db spring-petclinic
```

**Key observations:**
- Toolchain includes `java-core:0.1.0` — extractor provenance confirmed
- 332 resolved edges, 1396 unresolved edges
- All reliability levels: LOW
- Triggered downgrades: `alias_resolution_suspicion`, `missing_entrypoint_declarations`

**Unresolved edge breakdown (from trust categories):**
| Category | Count |
|----------|-------|
| CALLS obj.method (needs type info) | 652 |
| IMPORTS (file not found) | 427 |
| CALLS function (ambiguous or missing) | 252 |
| INSTANTIATES (class not found) | 36 |
| CALLS this.*.method (needs type info) | 15 |
| IMPLEMENTS (interface not found) | 14 |

### 2.3 Stats

```bash
rmap stats /tmp/petclinic.db spring-petclinic
```

**Observations:**
- 11 modules discovered (directory-based)
- All modules have `fan_in: 0`, `fan_out: 0`
- All modules have `instability: 0.0`, `distance_from_main_sequence: 1.0`
- File counts correct (e.g., `owner` module: 12 files)

### 2.4 Callers (with stable key)

```bash
rmap callers /tmp/petclinic.db spring-petclinic \
  "spring-petclinic:src/main/java/org/springframework/samples/petclinic/model/Person.java#Person.getLastName:SYMBOL:METHOD"
```

**Result:** 8 callers found
- `OwnerController.processFindForm` (production code)
- `Owner.toString` (production code)
- Multiple test methods (`VetTests.serialization`, `ClinicServiceTests.*`)

**Verified:** Cross-file call graph traversal works for Java methods.

### 2.5 Callees (with stable key)

```bash
rmap callees /tmp/petclinic.db spring-petclinic \
  "spring-petclinic:src/main/java/org/springframework/samples/petclinic/owner/OwnerController.java#OwnerController.processFindForm:SYMBOL:METHOD"
```

**Result:** 3 callees found
- `OwnerController.findPaginatedForOwnersLastName` (same-class call)
- `BaseEntity.getId` (cross-class call)
- `Person.getLastName` (cross-class call)

**Verified:** Internal method calls resolve across class boundaries.

### 2.6 Dead Code

```bash
rmap dead /tmp/petclinic.db spring-petclinic
```

**Result:** 302 dead symbols reported

**Sample categories:**
- Test constants (true dead): `I18nPropertiesSyncTest.BASE_NAME`
- Framework-managed classes (false positives): `CacheConfiguration`, `CrashController`
- Base classes without direct callers: `BaseEntity`

**Trust report correctly warns:** "Dead-code reliability is LOW on this repo."

### 2.7 Orient

```bash
rmap orient /tmp/petclinic.db spring-petclinic --budget small
```

**Result:**
- Schema: `rgr.agent.v1`
- Focus resolved as `repo`
- Confidence: `medium`
- Languages detected: `["java"]`
- File count: 47, Symbol count: 290

**Limits reported:**
- `DEAD_CODE_UNRELIABLE` — correctly suppressed
- `MODULE_DATA_UNAVAILABLE` — product gap (see Section 6)
- `GATE_NOT_CONFIGURED` — no requirement declarations

### 2.8 Modules List

```bash
rmap modules list /tmp/petclinic.db spring-petclinic
```

**Result:** Empty results (`count: 0`)

**Finding:** CLI gap — discovered modules visible in trust report but not in
`modules list` output. See Section 6.

## 3. Verified Capabilities

| Capability | Status | Evidence |
|------------|--------|----------|
| Java files indexed | PASS | 47 files indexed |
| Java FILE nodes created | PASS | Stable keys contain `.java:FILE` |
| Java SYMBOL nodes created | PASS | CLASS, INTERFACE, METHOD, CONSTRUCTOR, PROPERTY subtypes present |
| Symbols queryable by stable key | PASS | callers/callees resolve with full stable keys |
| CALLS edges extracted | PASS | 250 resolved CALLS, 919 unresolved |
| Cross-class CALLS resolve | PASS | `Person.getLastName` called from `OwnerController` |
| Trust includes java-core provenance | PASS | `toolchain.extractors` contains `java-core:0.1.0` |
| Orient detects Java | PASS | `languages: ["java"]` in orient output |

### Non-CLI Supporting Evidence

| Capability | Status | Evidence Source |
|------------|--------|-----------------|
| Metrics persisted | PASS | Integration tests (`index_java_persists_metrics`) — no CLI surface exists |

## 4. Expected Degradations

These are **by design** in v1 and documented in the design doc.

| Degradation | Cause | Impact |
|-------------|-------|--------|
| IMPORTS unresolved (427) | Package-qualified keys (`java.util.List`) don't map to file paths | Module connectivity zero, import graph unusable |
| CALLS obj.method unresolved (652) | Receiver type unknown without type resolver | Call graph incomplete for OOP patterns |
| IMPLEMENTS/EXTENDS unresolved (14) | External interfaces not in codebase | Inheritance graph incomplete |
| INSTANTIATES unresolved (36) | External classes not in codebase | Object creation graph incomplete |
| Module fan_in/fan_out zero | No resolved IMPORTS edges | Module metrics non-functional |

## 5. Framework-Specific Considerations

spring-petclinic uses Spring Boot with:
- `@Controller`, `@Service`, `@Repository` annotations
- JPA repositories extending Spring Data interfaces
- Configuration classes with `@Configuration`, `@Bean`

**Observations:**
- Spring-managed classes appear as dead code (no explicit callers)
- Framework-liveness inference not yet implemented for Spring annotations
- Repository interface methods have no callees (implementations are dynamic proxies)

## 6. Product Gaps Discovered

| Gap | Description | Severity |
|-----|-------------|----------|
| `modules list` empty | Discovered modules visible in `trust` and `stats` but not in `modules list` | Medium |
| No metrics CLI surface | Metrics persisted but no `rmap metrics` or similar command to query them | Low |
| Ambiguous symbol UX | Bare name queries fail with ambiguity error; must use full stable key | Low |
| No IMPLEMENTS edge query | Cannot query which classes implement an interface via CLI | Low |

## 7. Maturity Assessment

| Component | Maturity | Rationale |
|-----------|----------|-----------|
| Java extractor support module | PROTOTYPE→MATURE boundary | Core extraction works; edge resolution structurally limited |
| Integrated Java indexing | PROTOTYPE→MATURE boundary | End-to-end pipeline functional; integration tests pass |
| Java orientation product surface | PROTOTYPE | Unresolved imports degrade graph coherence fundamentally |

**Overall:** Java extractor v1 is integrated and initially validated on
spring-petclinic with known structural degradation in imports and
type-dependent calls.

## 8. Next Slice Recommendations

1. **Java import resolution (Phase D):** Map package imports to file paths
   using classpath/sourcepath resolution. Blocking for module connectivity.

2. **Spring framework-liveness inference:** Suppress `@Controller`,
   `@Service`, `@Repository`, `@Configuration` classes from dead-code.

3. **Metrics CLI surface:** Add `rmap metrics <db> <repo>` to query persisted
   complexity measurements.

4. **modules list parity:** Investigate why discovered modules don't appear in
   `modules list` (likely MODULE kind node query vs derived module rollup
   discrepancy).

## 9. Validation Method Notes

- Primary validation performed via CLI commands
- No raw SQL used for product validation
- Stable keys used for all symbol queries (no bare names)
- Trust report used as primary quality signal
- Metrics persistence verified via integration tests (no CLI surface exists)

**Allowed SQL fallback:** Not invoked. Product-facing validation questions
answerable via CLI surfaces (with gaps documented in Section 6).

**Non-CLI evidence:** Metrics persistence claim relies on integration test
evidence, not CLI observation. This is documented separately in Section 3.
