/**
 * rgr boundary <repo> — boundary interaction query surface.
 *
 * Explicit commands for querying boundary facts and derived links.
 * Boundary data is NOT mixed into core graph commands (callers,
 * callees, path, dead). It has its own query surface.
 *
 * Subcommands:
 *   rgr boundary providers <repo>   — list provider facts
 *   rgr boundary consumers <repo>   — list consumer facts
 *   rgr boundary links <repo>       — list matched provider-consumer links
 *   rgr boundary summary <repo>     — aggregate counts
 *   rgr boundary unmatched <repo>   — providers/consumers with no match
 */

import { resolve as resolvePath } from "node:path";
import type { Command } from "commander";
import type { AppContext } from "../../main.js";

export function registerBoundaryCommands(
	program: Command,
	getCtx: () => AppContext,
): void {
	const boundary = program
		.command("boundary")
		.description("Boundary interaction facts and links");

	// ── summary ─────────────────────────────────────────────────────

	boundary
		.command("summary <repo>")
		.description("Aggregate boundary fact and link counts")
		.option("--json", "JSON output")
		.action((repoRef: string, opts: { json?: boolean }) => {
			const ctx = getCtx();
			const resolved = resolveRepoAndSnapshot(ctx, repoRef);
			if (!resolved) {
				outputError(opts.json, `Repository not found or not indexed: ${repoRef}`);
				process.exitCode = 1;
				return;
			}
			const { repo, snapshotUid } = resolved;

			const providers = ctx.storage.queryBoundaryProviderFacts({ snapshotUid });
			const consumers = ctx.storage.queryBoundaryConsumerFacts({ snapshotUid });
			const links = ctx.storage.queryBoundaryLinks({ snapshotUid });

			// Count by mechanism.
			const mechProviders = countBy(providers, (p) => p.mechanism);
			const mechConsumers = countBy(consumers, (c) => c.mechanism);
			const mechLinks = countBy(links, (l) => l.providerMechanism);

			// Match rates: count unique provider/consumer facts that appear in links.
			const providerLinked = new Set<string>();
			const consumerLinked = new Set<string>();
			// Need to query links for UIDs — the BoundaryLinkRow doesn't expose fact UIDs directly.
			// Use provider/consumer source files as proxy for counting.
			// For accurate counts, count unique (providerOp+providerFile) and (consumerOp+consumerFile).
			for (const l of links) {
				providerLinked.add(`${l.providerOperation}|${l.providerSourceFile}|${l.providerLineStart}`);
				consumerLinked.add(`${l.consumerOperation}|${l.consumerSourceFile}|${l.consumerLineStart}`);
			}

			if (opts.json) {
				console.log(JSON.stringify({
					command: "boundary summary",
					repo: repo.name,
					snapshot_uid: snapshotUid,
					providers: providers.length,
					consumers: consumers.length,
					links: links.length,
					provider_match_rate: providers.length > 0
						? `${((providerLinked.size / providers.length) * 100).toFixed(1)}%`
						: "n/a",
					consumer_match_rate: consumers.length > 0
						? `${((consumerLinked.size / consumers.length) * 100).toFixed(1)}%`
						: "n/a",
					by_mechanism: {
						providers: Object.fromEntries(mechProviders),
						consumers: Object.fromEntries(mechConsumers),
						links: Object.fromEntries(mechLinks),
					},
				}, null, 2));
			} else {
				console.log(`Boundary summary for ${repo.name}`);
				console.log(`  Providers: ${providers.length}`);
				console.log(`  Consumers: ${consumers.length}`);
				console.log(`  Links:     ${links.length}`);
				if (providers.length > 0) {
					console.log(`  Provider match rate: ${((providerLinked.size / providers.length) * 100).toFixed(1)}%`);
				}
				if (consumers.length > 0) {
					console.log(`  Consumer match rate: ${((consumerLinked.size / consumers.length) * 100).toFixed(1)}%`);
				}
				// Collect all unique mechanisms from providers and consumers.
				const allMechanisms = new Set([...mechProviders.keys(), ...mechConsumers.keys()]);
				if (allMechanisms.size > 0) {
					console.log("\n  By mechanism:");
					for (const mech of [...allMechanisms].sort()) {
						const pCount = mechProviders.get(mech) ?? 0;
						const cCount = mechConsumers.get(mech) ?? 0;
						const lCount = mechLinks.get(mech) ?? 0;
						console.log(`    ${mech}: ${pCount} providers, ${cCount} consumers, ${lCount} links`);
					}
				}
			}
		});

	// ── providers ────────────────────────────────────────────────────

	boundary
		.command("providers <repo>")
		.description("List boundary provider facts")
		.option("--json", "JSON output")
		.option("--mechanism <type>", "Filter by mechanism (http, grpc, ...)")
		.option("--file <path>", "Filter by source file")
		.option("--limit <n>", "Max results", "100")
		.action((repoRef: string, opts: { json?: boolean; mechanism?: string; file?: string; limit: string }) => {
			const ctx = getCtx();
			const resolved = resolveRepoAndSnapshot(ctx, repoRef);
			if (!resolved) {
				outputError(opts.json, `Repository not found or not indexed: ${repoRef}`);
				process.exitCode = 1;
				return;
			}
			const { repo, snapshotUid } = resolved;
			const limit = Number.parseInt(opts.limit, 10);

			const providers = ctx.storage.queryBoundaryProviderFacts({
				snapshotUid,
				mechanism: opts.mechanism as any,
				sourceFile: opts.file,
				limit: Number.isFinite(limit) ? limit : undefined,
			});

			if (opts.json) {
				console.log(JSON.stringify({
					command: "boundary providers",
					repo: repo.name,
					count: providers.length,
					results: providers.map((p) => ({
						mechanism: p.mechanism,
						operation: p.operation,
						address: p.address,
						matcher_key: p.matcherKey,
						handler: p.handlerStableKey,
						source_file: p.sourceFile,
						line: p.lineStart,
						framework: p.framework,
						basis: p.basis,
					})),
				}, null, 2));
			} else {
				if (providers.length === 0) {
					console.log("No boundary provider facts found.");
					return;
				}
				console.log(`OPERATION${" ".repeat(51)}SOURCE FILE${" ".repeat(29)}LINE  FRAMEWORK`);
				for (const p of providers) {
					const op = p.operation.padEnd(60).slice(0, 60);
					const file = p.sourceFile.padEnd(40).slice(0, 40);
					console.log(`${op}${file}${String(p.lineStart).padStart(5)}  ${p.framework}`);
				}
				console.log(`\n${providers.length} provider fact(s)`);
			}
		});

	// ── consumers ────────────────────────────────────────────────────

	boundary
		.command("consumers <repo>")
		.description("List boundary consumer facts")
		.option("--json", "JSON output")
		.option("--mechanism <type>", "Filter by mechanism (http, grpc, ...)")
		.option("--file <path>", "Filter by source file")
		.option("--limit <n>", "Max results", "100")
		.action((repoRef: string, opts: { json?: boolean; mechanism?: string; file?: string; limit: string }) => {
			const ctx = getCtx();
			const resolved = resolveRepoAndSnapshot(ctx, repoRef);
			if (!resolved) {
				outputError(opts.json, `Repository not found or not indexed: ${repoRef}`);
				process.exitCode = 1;
				return;
			}
			const { repo, snapshotUid } = resolved;
			const limit = Number.parseInt(opts.limit, 10);

			const consumers = ctx.storage.queryBoundaryConsumerFacts({
				snapshotUid,
				mechanism: opts.mechanism as any,
				sourceFile: opts.file,
				limit: Number.isFinite(limit) ? limit : undefined,
			});

			if (opts.json) {
				console.log(JSON.stringify({
					command: "boundary consumers",
					repo: repo.name,
					count: consumers.length,
					results: consumers.map((c) => ({
						mechanism: c.mechanism,
						operation: c.operation,
						address: c.address,
						matcher_key: c.matcherKey,
						caller: c.callerStableKey,
						source_file: c.sourceFile,
						line: c.lineStart,
						basis: c.basis,
						confidence: c.confidence,
					})),
				}, null, 2));
			} else {
				if (consumers.length === 0) {
					console.log("No boundary consumer facts found.");
					return;
				}
				console.log(`OPERATION${" ".repeat(51)}SOURCE FILE${" ".repeat(29)}LINE  CONF  BASIS`);
				for (const c of consumers) {
					const op = c.operation.padEnd(60).slice(0, 60);
					const file = c.sourceFile.padEnd(40).slice(0, 40);
					console.log(`${op}${file}${String(c.lineStart).padStart(5)}  ${c.confidence.toFixed(2)}  ${c.basis}`);
				}
				console.log(`\n${consumers.length} consumer fact(s)`);
			}
		});

	// ── links ────────────────────────────────────────────────────────

	boundary
		.command("links <repo>")
		.description("List matched boundary links (derived)")
		.option("--json", "JSON output")
		.option("--mechanism <type>", "Filter by mechanism")
		.option("--limit <n>", "Max results", "100")
		.action((repoRef: string, opts: { json?: boolean; mechanism?: string; limit: string }) => {
			const ctx = getCtx();
			const resolved = resolveRepoAndSnapshot(ctx, repoRef);
			if (!resolved) {
				outputError(opts.json, `Repository not found or not indexed: ${repoRef}`);
				process.exitCode = 1;
				return;
			}
			const { repo, snapshotUid } = resolved;
			const limit = Number.parseInt(opts.limit, 10);

			const links = ctx.storage.queryBoundaryLinks({
				snapshotUid,
				mechanism: opts.mechanism as any,
				limit: Number.isFinite(limit) ? limit : undefined,
			});

			if (opts.json) {
				console.log(JSON.stringify({
					command: "boundary links",
					repo: repo.name,
					count: links.length,
					results: links.map((l) => ({
						provider_operation: l.providerOperation,
						provider_file: l.providerSourceFile,
						provider_line: l.providerLineStart,
						provider_framework: l.providerFramework,
						consumer_operation: l.consumerOperation,
						consumer_file: l.consumerSourceFile,
						consumer_line: l.consumerLineStart,
						match_basis: l.matchBasis,
						confidence: l.confidence,
					})),
				}, null, 2));
			} else {
				if (links.length === 0) {
					console.log("No boundary links found.");
					return;
				}
				console.log("PROVIDER                                             CONSUMER                                             BASIS           CONF");
				for (const l of links) {
					const prov = l.providerOperation.padEnd(53).slice(0, 53);
					const cons = l.consumerOperation.padEnd(53).slice(0, 53);
					console.log(`${prov}${cons}${l.matchBasis.padEnd(16)}${l.confidence.toFixed(2)}`);
				}
				console.log(`\n${links.length} link(s)`);
			}
		});

	// ── unmatched ────────────────────────────────────────────────────

	boundary
		.command("unmatched <repo>")
		.description("List providers/consumers with no matched link")
		.option("--json", "JSON output")
		.option("--side <side>", "Filter: 'providers' or 'consumers'")
		.option("--limit <n>", "Max results", "50")
		.action((repoRef: string, opts: { json?: boolean; side?: string; limit: string }) => {
			const ctx = getCtx();
			const resolved = resolveRepoAndSnapshot(ctx, repoRef);
			if (!resolved) {
				outputError(opts.json, `Repository not found or not indexed: ${repoRef}`);
				process.exitCode = 1;
				return;
			}
			const { repo, snapshotUid } = resolved;
			const limit = Number.parseInt(opts.limit, 10) || 50;

			const providers = ctx.storage.queryBoundaryProviderFacts({ snapshotUid });
			const consumers = ctx.storage.queryBoundaryConsumerFacts({ snapshotUid });
			const links = ctx.storage.queryBoundaryLinks({ snapshotUid });

			// Build sets of linked operations for matching.
			const linkedProvKeys = new Set(
				links.map((l) => `${l.providerOperation}|${l.providerSourceFile}|${l.providerLineStart}`),
			);
			const linkedConsKeys = new Set(
				links.map((l) => `${l.consumerOperation}|${l.consumerSourceFile}|${l.consumerLineStart}`),
			);

			const unmatchedProviders = providers.filter(
				(p) => !linkedProvKeys.has(`${p.operation}|${p.sourceFile}|${p.lineStart}`),
			);
			const unmatchedConsumers = consumers.filter(
				(c) => !linkedConsKeys.has(`${c.operation}|${c.sourceFile}|${c.lineStart}`),
			);

			const showProviders = !opts.side || opts.side === "providers";
			const showConsumers = !opts.side || opts.side === "consumers";

			if (opts.json) {
				const result: Record<string, unknown> = {
					command: "boundary unmatched",
					repo: repo.name,
				};
				if (showProviders) {
					result.unmatched_providers = {
						count: unmatchedProviders.length,
						results: unmatchedProviders.slice(0, limit).map((p) => ({
							operation: p.operation,
							source_file: p.sourceFile,
							line: p.lineStart,
							framework: p.framework,
						})),
					};
				}
				if (showConsumers) {
					result.unmatched_consumers = {
						count: unmatchedConsumers.length,
						results: unmatchedConsumers.slice(0, limit).map((c) => ({
							operation: c.operation,
							source_file: c.sourceFile,
							line: c.lineStart,
							basis: c.basis,
						})),
					};
				}
				console.log(JSON.stringify(result, null, 2));
			} else {
				if (showProviders) {
					console.log(`Unmatched providers (${unmatchedProviders.length}):`);
					if (unmatchedProviders.length === 0) {
						console.log("  (none)");
					} else {
						for (const p of unmatchedProviders.slice(0, limit)) {
							console.log(`  ${p.operation.padEnd(55).slice(0, 55)} ${p.sourceFile}`);
						}
					}
				}
				if (showConsumers) {
					if (showProviders) console.log();
					console.log(`Unmatched consumers (${unmatchedConsumers.length}):`);
					if (unmatchedConsumers.length === 0) {
						console.log("  (none)");
					} else {
						for (const c of unmatchedConsumers.slice(0, limit)) {
							console.log(`  ${c.operation.padEnd(55).slice(0, 55)} ${c.sourceFile}`);
						}
					}
				}
			}
		});
}

// ── Helpers ─────────────────────────────────────────────────────────

function resolveRepoAndSnapshot(
	ctx: AppContext,
	repoRef: string,
): { repo: { repoUid: string; name: string }; snapshotUid: string } | null {
	const repo =
		ctx.storage.getRepo({ uid: repoRef }) ??
		ctx.storage.getRepo({ name: repoRef }) ??
		ctx.storage.getRepo({ rootPath: resolvePath(repoRef) });
	if (!repo) return null;

	const snapshot = ctx.storage.getLatestSnapshot(repo.repoUid);
	if (!snapshot) return null;

	return { repo, snapshotUid: snapshot.snapshotUid };
}

function outputError(json: boolean | undefined, message: string): void {
	if (json) {
		console.log(JSON.stringify({ error: message }));
	} else {
		console.error(`Error: ${message}`);
	}
}

function countBy<T>(items: T[], key: (item: T) => string): Map<string, number> {
	const map = new Map<string, number>();
	for (const item of items) {
		const k = key(item);
		map.set(k, (map.get(k) ?? 0) + 1);
	}
	return map;
}
