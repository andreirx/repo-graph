import { configDefaults, defineConfig } from "vitest/config";

// ── Shared test options ────────────────────────────────────────────
//
// `sharedTestOptions` is the single source of truth for vitest test
// settings that must be identical between the default suite config
// (this file) and the live integration suite config
// (`vitest.live.config.ts`). Both configs spread this object into
// their `test` block and then layer their surface-specific
// `include` / `exclude` on top.
//
// Why a named export, not `mergeConfig`: vitest's (Vite's)
// `mergeConfig` concatenates arrays during deep-merge. That is the
// wrong semantics for `exclude` — the live config needs to REPLACE
// the default config's exclude (so it does not re-inherit the
// `test/live/**` exclusion), not append to it. Spreading a plain
// shared options object lets each config own its `exclude` outright
// while still inheriting the rest of the shared settings.
//
// Add future shared settings (plugins, setupFiles, aliases,
// coverage, environment, globals, etc.) to this object. Both
// configs will inherit them automatically.
export const sharedTestOptions = {
  passWithNoTests: true,
};

export default defineConfig({
  test: {
    ...sharedTestOptions,
    // Exclude live external-tool integration tests from the default
    // suite. They live under `test/live/` (jdtls, rust-analyzer) and
    // are opt-in only via `pnpm run test:live` (which loads
    // `vitest.live.config.ts`, a separate config file with NO
    // `test/live/**` exclusion). The default surface
    // (`pnpm run test`, `pnpm run test:all`) MUST be deterministic
    // and runnable without external language servers on PATH.
    //
    // See docs/TECH-DEBT.md → "Live jdtls test self-skip is incomplete
    // (P2) — RESOLVED in D-1" for the historical context.
    //
    // `configDefaults.exclude` carries vitest's standard exclusions
    // (node_modules, dist, .git, etc.). We spread it rather than
    // replace it, otherwise vitest would re-discover tests inside
    // build artifact directories.
    exclude: [...configDefaults.exclude, "test/live/**"],
  },
});
