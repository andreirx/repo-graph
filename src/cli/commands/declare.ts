import { resolve } from "node:path";
import type { Command } from "commander";
import { v4 as uuidv4 } from "uuid";
import { parseDeclarationValue } from "../../core/model/declaration.js";
import { DeclarationKind } from "../../core/model/index.js";
import type { AppContext } from "../../main.js";
import { buildAuthoredBasisJson } from "../../version.js";

export function registerDeclareCommands(
	program: Command,
	getCtx: () => AppContext,
): void {
	const declare = program
		.command("declare")
		.description("Manage human-supplied declarations");

	declare
		.command("module <repo> <modulePath>")
		.description("Declare metadata about a module")
		.option("--purpose <text>", "Module purpose")
		.option("--maturity <level>", "PROTOTYPE | MATURE | PRODUCTION")
		.option("--owner <text>", "Team or person responsible")
		.option("--json", "JSON output")
		.action(
			(
				repoRef: string,
				modulePath: string,
				opts: {
					purpose?: string;
					maturity?: string;
					owner?: string;
					json?: boolean;
				},
			) => {
				const ctx = getCtx();
				const repo = resolveRepo(ctx, repoRef);
				if (!repo) {
					outputError(opts.json, `Repository not found: ${repoRef}`);
					process.exitCode = 1;
					return;
				}

				const stableKey = `${repo.repoUid}:${modulePath}:MODULE`;
				const value = {
					...(opts.purpose && { purpose: opts.purpose }),
					...(opts.maturity && { maturity: opts.maturity }),
					...(opts.owner && { owner: opts.owner }),
				};

				// Validate (module has all-optional fields but maturity is constrained)
				parseDeclarationValue(DeclarationKind.MODULE, JSON.stringify(value));

				const uid = insertDeclaration(
					ctx,
					repo.repoUid,
					stableKey,
					DeclarationKind.MODULE,
					value,
				);
				outputCreated(opts.json, uid, "module", stableKey);
			},
		);

	declare
		.command("boundary <repo> <moduleA>")
		.description("Declare a forbidden dependency")
		.requiredOption(
			"--forbids <moduleB>",
			"Module that must not be depended on",
		)
		.option("--reason <text>", "Why this boundary exists")
		.option("--json", "JSON output")
		.action(
			(
				repoRef: string,
				moduleA: string,
				opts: {
					forbids: string;
					reason?: string;
					json?: boolean;
				},
			) => {
				const ctx = getCtx();
				const repo = resolveRepo(ctx, repoRef);
				if (!repo) {
					outputError(opts.json, `Repository not found: ${repoRef}`);
					process.exitCode = 1;
					return;
				}

				const stableKey = `${repo.repoUid}:${moduleA}:MODULE`;
				const value = {
					forbids: opts.forbids,
					...(opts.reason && { reason: opts.reason }),
				};

				// Validate
				parseDeclarationValue(DeclarationKind.BOUNDARY, JSON.stringify(value));

				const uid = insertDeclaration(
					ctx,
					repo.repoUid,
					stableKey,
					DeclarationKind.BOUNDARY,
					value,
				);
				outputCreated(opts.json, uid, "boundary", stableKey);
			},
		);

	declare
		.command("entrypoint <repo> <symbol>")
		.description("Declare a symbol as a known entrypoint")
		.option(
			"--type <type>",
			"route_handler | event_listener | cron_job | public_export",
			"public_export",
		)
		.option("--description <text>", "Description")
		.option("--json", "JSON output")
		.action(
			(
				repoRef: string,
				symbol: string,
				opts: {
					type: string;
					description?: string;
					json?: boolean;
				},
			) => {
				const ctx = getCtx();
				const repo = resolveRepo(ctx, repoRef);
				if (!repo) {
					outputError(opts.json, `Repository not found: ${repoRef}`);
					process.exitCode = 1;
					return;
				}

				const value = {
					type: opts.type,
					...(opts.description && { description: opts.description }),
				};

				// Validate
				parseDeclarationValue(
					DeclarationKind.ENTRYPOINT,
					JSON.stringify(value),
				);

				const uid = insertDeclaration(
					ctx,
					repo.repoUid,
					symbol,
					DeclarationKind.ENTRYPOINT,
					value,
				);
				outputCreated(opts.json, uid, "entrypoint", symbol);
			},
		);

	declare
		.command("invariant <repo> <symbol> <text>")
		.description("Attach a business invariant to a symbol")
		.option("--severity <level>", "critical | high | medium | low", "medium")
		.option("--json", "JSON output")
		.action(
			(
				repoRef: string,
				symbol: string,
				text: string,
				opts: {
					severity: string;
					json?: boolean;
				},
			) => {
				const ctx = getCtx();
				const repo = resolveRepo(ctx, repoRef);
				if (!repo) {
					outputError(opts.json, `Repository not found: ${repoRef}`);
					process.exitCode = 1;
					return;
				}

				const value = { text, severity: opts.severity };

				// Validate
				parseDeclarationValue(DeclarationKind.INVARIANT, JSON.stringify(value));

				const uid = insertDeclaration(
					ctx,
					repo.repoUid,
					symbol,
					DeclarationKind.INVARIANT,
					value,
				);
				outputCreated(opts.json, uid, "invariant", symbol);
			},
		);

	declare
		.command("list <repo>")
		.description("List all active declarations")
		.option("--kind <kind>", "Filter by declaration kind")
		.option("--json", "JSON output")
		.action((repoRef: string, opts: { kind?: string; json?: boolean }) => {
			const ctx = getCtx();
			const repo = resolveRepo(ctx, repoRef);
			if (!repo) {
				outputError(opts.json, `Repository not found: ${repoRef}`);
				process.exitCode = 1;
				return;
			}

			const decls = ctx.storage.getActiveDeclarations({
				repoUid: repo.repoUid,
				kind: opts.kind,
			});

			if (opts.json) {
				console.log(
					JSON.stringify(
						decls.map((d) => ({
							declaration_uid: d.declarationUid,
							target: d.targetStableKey,
							kind: d.kind,
							value: JSON.parse(d.valueJson),
							created_at: d.createdAt,
						})),
					),
				);
			} else {
				if (decls.length === 0) {
					console.log("No active declarations.");
					return;
				}
				for (const d of decls) {
					console.log(`[${d.kind}] ${d.targetStableKey}`);
					console.log(`  ${d.valueJson}`);
					console.log(`  uid: ${d.declarationUid}`);
				}
			}
		});

	declare
		.command("remove <repo> <declarationUid>")
		.description("Deactivate a declaration by UID")
		.option("--json", "JSON output")
		.action(
			(repoRef: string, declarationUid: string, opts: { json?: boolean }) => {
				const ctx = getCtx();
				const repo = resolveRepo(ctx, repoRef);
				if (!repo) {
					outputError(opts.json, `Repository not found: ${repoRef}`);
					process.exitCode = 1;
					return;
				}

				ctx.storage.deactivateDeclaration(declarationUid);

				if (opts.json) {
					console.log(JSON.stringify({ deactivated: declarationUid }));
				} else {
					console.log(`Deactivated declaration ${declarationUid}`);
				}
			},
		);
}

// ── Helpers ────────────────────────────────────────────────────────────

function resolveRepo(ctx: AppContext, ref: string) {
	return (
		ctx.storage.getRepo({ uid: ref }) ??
		ctx.storage.getRepo({ name: ref }) ??
		ctx.storage.getRepo({ rootPath: resolve(ref) })
	);
}

function insertDeclaration(
	ctx: AppContext,
	repoUid: string,
	targetStableKey: string,
	kind: DeclarationKind,
	value: Record<string, unknown>,
): string {
	const uid = uuidv4();
	ctx.storage.insertDeclaration({
		declarationUid: uid,
		repoUid,
		snapshotUid: null,
		targetStableKey,
		kind,
		valueJson: JSON.stringify(value),
		createdAt: new Date().toISOString(),
		createdBy: "cli",
		supersedesUid: null,
		isActive: true,
		authoredBasisJson: JSON.stringify(buildAuthoredBasisJson()),
	});
	return uid;
}

function outputCreated(
	json: boolean | undefined,
	uid: string,
	kind: string,
	target: string,
): void {
	if (json) {
		console.log(JSON.stringify({ declaration_uid: uid, kind, target }));
	} else {
		console.log(`Created ${kind} declaration ${uid} for ${target}`);
	}
}

function outputError(json: boolean | undefined, message: string): void {
	if (json) {
		console.log(JSON.stringify({ error: message }));
	} else {
		console.error(`Error: ${message}`);
	}
}
