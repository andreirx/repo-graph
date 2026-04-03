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
		.command("requirement <repo> <module>")
		.description(
			"Declare a structured requirement (objective, constraints, non-goals)",
		)
		.requiredOption("--req-id <id>", "Requirement identifier (e.g. REQ-001)")
		.requiredOption("--objective <text>", "What must be achieved")
		.option("--constraint <text...>", "What must not happen (repeatable)")
		.option("--non-goal <text...>", "Explicitly out of scope (repeatable)")
		.option("--json", "JSON output")
		.action(
			(
				repoRef: string,
				modulePath: string,
				opts: {
					reqId: string;
					objective: string;
					constraint?: string[];
					nonGoal?: string[];
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
					req_id: opts.reqId,
					version: 1,
					objective: opts.objective,
					...(opts.constraint && { constraints: opts.constraint }),
					...(opts.nonGoal && { non_goals: opts.nonGoal }),
					status: "active",
				};

				parseDeclarationValue(
					DeclarationKind.REQUIREMENT,
					JSON.stringify(value),
				);

				const uid = insertDeclaration(
					ctx,
					repo.repoUid,
					stableKey,
					DeclarationKind.REQUIREMENT,
					value,
				);
				outputCreated(opts.json, uid, "requirement", stableKey);
			},
		);

	declare
		.command("obligation <repo> <reqId>")
		.description("Add a verification obligation to an existing requirement")
		.requiredOption("--obligation <text>", "What must be proven")
		.requiredOption(
			"--method <method>",
			"Verification method (arch_violations|coverage_threshold|complexity_threshold|hotspot_threshold)",
		)
		.option("--target <path>", "Target module or file path")
		.option("--threshold <n>", "Numeric threshold")
		.option("--operator <op>", "Comparison operator (>= | <= | == | > | <)")
		.option("--json", "JSON output")
		.action(
			(
				repoRef: string,
				reqId: string,
				opts: {
					obligation: string;
					method: string;
					target?: string;
					threshold?: string;
					operator?: string;
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

				// Find the active requirement with this req_id
				const allReqs = ctx.storage.getActiveDeclarations({
					repoUid: repo.repoUid,
					kind: DeclarationKind.REQUIREMENT,
				});
				const reqDecl = allReqs.find((d) => {
					const val = JSON.parse(d.valueJson) as Record<string, unknown>;
					return val.req_id === reqId;
				});

				if (!reqDecl) {
					outputError(
						opts.json,
						`No active requirement found with req_id "${reqId}"`,
					);
					process.exitCode = 1;
					return;
				}

				// Parse current value and add the obligation
				const currentValue = JSON.parse(reqDecl.valueJson) as Record<
					string,
					unknown
				>;
				const existingObligations = (currentValue.verification ?? []) as Array<
					Record<string, unknown>
				>;

				const newObligation: Record<string, unknown> = {
					obligation: opts.obligation,
					method: opts.method,
				};
				if (opts.target) newObligation.target = opts.target;
				if (opts.threshold)
					newObligation.threshold = Number.parseFloat(opts.threshold);
				if (opts.operator) newObligation.operator = opts.operator;

				existingObligations.push(newObligation);
				currentValue.verification = existingObligations;
				currentValue.version = ((currentValue.version as number) ?? 0) + 1;

				// Supersede the old declaration with updated value.
				// Link the new row back to the old one for audit lineage.
				ctx.storage.deactivateDeclaration(reqDecl.declarationUid);
				const uid = insertDeclaration(
					ctx,
					repo.repoUid,
					reqDecl.targetStableKey,
					DeclarationKind.REQUIREMENT,
					currentValue,
					reqDecl.declarationUid,
				);

				if (opts.json) {
					console.log(
						JSON.stringify({
							declaration_uid: uid,
							req_id: reqId,
							obligation: opts.obligation,
							method: opts.method,
							version: currentValue.version,
						}),
					);
				} else {
					console.log(
						`Added obligation to ${reqId} (v${currentValue.version}): ${opts.obligation}`,
					);
				}
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
	supersedesUid?: string,
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
		supersedesUid: supersedesUid ?? null,
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
