import { defineConfig } from "vitest/config";
import { sharedTestOptions } from "./vitest.config.js";

// ── Live integration test config ──────────────────────────────────
//
// Dedicated vitest config for the live external-tool integration
// test surface (`test/live/`). Loaded explicitly by the
// `pnpm run test:live` script via `vitest run --config vitest.live.config.ts`.
//
// Why a separate config file rather than the default config plus a
// CLI filter: vitest applies the config `exclude` list globally,
// regardless of which directory the CLI filter targets. The default
// `vitest.config.ts` excludes `test/live/**` so the default suite
// (`pnpm run test`) does not run external-tool tests. That same
// exclusion would prevent `vitest run test/live` from finding any
// files. A second config with its own scope is the cleanest
// resolution. (Vitest projects / `--project` filtering is the
// alternative; D-1 chose this approach to avoid mutating the
// `test` script's shape.)
//
// This config inherits `sharedTestOptions` from `vitest.config.ts`
// so settings like `passWithNoTests` (and any future shared
// settings: plugins, setupFiles, aliases, coverage, environment)
// stay in sync between surfaces. ONLY the surface-specific
// `include` / `exclude` differ.
//
// Surface contract:
//   - includes only `test/live/**/*.test.ts`
//   - does NOT exclude `test/live/**` (that exclusion exists only
//     in the default config to keep live tests out of the default
//     suite)
//   - inherits no other exclude beyond vitest's hardcoded built-in
//     defaults (node_modules, .git, etc.) which vitest applies
//     internally regardless of `exclude` config
//
// Tests under `test/live/` self-skip when their required external
// tool (jdtls, rust-analyzer) is missing from PATH. This config
// does NOT enforce the tool presence — `test:live` is opt-in
// observability, not a mandatory acceptance surface. See
// `docs/TECH-DEBT.md` → "Live jdtls test self-skip is incomplete
// (P2) — RESOLVED in D-1" for the gating decision history.

export default defineConfig({
  test: {
    ...sharedTestOptions,
    include: ["test/live/**/*.test.ts"],
  },
});
