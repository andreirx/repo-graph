#!/usr/bin/env node

/**
 * rgr — Repo-Graph CLI
 *
 * Deterministic code graph tool for analyzing legacy codebases.
 * Produces structured JSON for AI agent consumption.
 */

import { Command } from "commander";
import { type AppContext, bootstrap, shutdown } from "../main.js";
import { registerArchCommands } from "./commands/arch.js";
import { registerDeclareCommands } from "./commands/declare.js";
import { registerGraphCommands } from "./commands/graph.js";
import { registerRepoCommands } from "./commands/repo.js";

const program = new Command();

program
	.name("rgr")
	.description("Deterministic code graph tool for analyzing legacy codebases")
	.version("0.1.0");

// Lazy-init context — only bootstrapped when a command actually runs.
// This avoids initializing the database and WASM parser for --help or --version.
let ctx: AppContext | null = null;

function getCtx(): AppContext {
	if (!ctx) {
		throw new Error("Application not initialized. This is an internal error.");
	}
	return ctx;
}

// Register command groups
registerRepoCommands(program, getCtx);
registerGraphCommands(program, getCtx);
registerDeclareCommands(program, getCtx);
registerArchCommands(program, getCtx);

// Main execution
async function main(): Promise<void> {
	// Bootstrap before parsing (needed for all commands except --help/--version)
	const args = process.argv.slice(2);
	const isHelpOrVersion =
		args.length === 0 ||
		args.includes("--help") ||
		args.includes("-h") ||
		args.includes("--version") ||
		args.includes("-V");

	if (!isHelpOrVersion) {
		ctx = await bootstrap();
	}

	try {
		await program.parseAsync(process.argv);
	} finally {
		if (ctx) {
			shutdown(ctx);
		}
	}
}

main().catch((err) => {
	console.error(err instanceof Error ? err.message : String(err));
	process.exitCode = 1;
});
