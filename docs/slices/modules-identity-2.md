# MODULES-IDENTITY-2 — twin module names disambiguate; absent detectors say so

Status: SPECIFIED (2026-09-01) · Track: Usefulness audit v0.11.0 fix queue, tail item.
CODE slice, small. Maturity: MATURE surfaces.

## 1. Problem (measured — audit run 2026-08-30T20-44-57Z, django)

- `modules list` shows two modules both named "Django" ("Django 87 files (24 test)" and
  "Django 907 files (1997 test)") with nothing to tell them apart — orient already got the
  manifest-path suffix (v0.9.0 fix #5 landed there); modules-list did not.
- `surfaces list` on django returns "0 surfaces … No recognized patterns" on a repo with
  hundreds of URLconf routes — no Django URL detector exists on this build, and the output
  blames the repo instead of stating the coverage gap (the same class RESOURCE-HONESTY-1
  just killed on resource-list).

## 2. Contract

1. **modules-list reuses orient's disambiguation**: when two modules share a display name,
   each renders its manifest path suffix (the SAME derivation orient uses — one
   implementation, shared or moved, never a second copy; if sharing requires a new crate
   edge, prefer moving the helper to the crate both consume, and record the move).
2. **surfaces-list zero-state states detector coverage** per the RESOURCE-HONESTY-1 pattern:
   which surface detectors this build ships (from their registry — one source of truth) and
   the honest no-detector sentence for materially-present frameworks/languages without one
   (django URLconf named as not-covered). No new detector in this slice.
3. JSON additive; exit codes unchanged.

## 3. Stop conditions

Frozen: module identity computation (rendering disambiguates; identity does not change),
storage schema, exit codes, trust. STANDING HONESTY RULES. New public APIs beyond additive
DTO fields → DECISION_REQUIRED (registry read-only queries follow the RESOURCE-HONESTY-1
ratification pattern — cite it if needed). Unmet DoD → STOP + DECISION_REQUIRED. Never touch
the operator's real state root. Do NOT commit.

## 4. Validation (SYNCHRONOUS; INCREMENTAL REPORT — binding)

- Unit: same-name disambiguation (fixture with twins); unique names unchanged (no suffix
  noise); surfaces zero-state coverage statement; shared-derivation test proving one
  implementation.
- Live proof (isolated state root, registry sha unchanged): django — modules-list twins
  disambiguated by manifest suffix; surfaces-list zero-state names the coverage gap without
  blaming the repo. glamCRM spot-check unchanged (its module names are unique; its surfaces
  non-zero).
- Chunked cargo gates; consolidation witness; dogfood-isolated green.

## 5. Definition of done

No two indistinguishable module rows anywhere; surfaces' empty answer states the tool's
coverage; both derive from existing facts/registries with no duplicated derivation; gates
green.
