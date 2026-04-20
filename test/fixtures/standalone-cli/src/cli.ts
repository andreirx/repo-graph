#!/usr/bin/env node

/**
 * Standalone CLI entrypoint.
 * No workspace, no monorepo — just a single package with bin entry.
 */

export function main(): void {
	console.log("Hello from standalone CLI");
}

main();
