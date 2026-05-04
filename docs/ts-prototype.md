# TypeScript Prototype (Legacy)

The TypeScript CLI (`rgr`) was the original prototype. It remains for parity checks and features not yet ported to Rust, but is not the primary development path.

## Build and Test

```bash
pnpm install
pnpm run build              # Build TypeScript
pnpm run test               # TS tests (includes CLI integration)
pnpm run test:all           # Full TS + Rust acceptance
```

## Lint

```bash
pnpm run lint               # Run Biome linter
pnpm run lint:fix           # Auto-fix
```

## Native Dependency Note

`better-sqlite3` is keyed to Node ABI. If you switch Node versions:
```bash
nvm use && pnpm rebuild better-sqlite3
```

## Conventions

- Use `pnpm` not npm.
- `rgr` (TS) supports `--json` with human-readable default.

## CLI Reference

See `docs/cli/v1-cli.txt` for the TypeScript CLI contract.
