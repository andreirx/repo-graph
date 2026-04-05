/**
 * rgr docs <target> — read-only provisional annotations surface.
 *
 * Resolves a query string to exactly one target via the 3-step
 * precedence in annotations-contract.txt §6 and returns annotations
 * attached to that target.
 *
 * HARD RULE: this is the ONLY command that reads annotations. Any
 * future command that wants to surface annotation content MUST
 * either compose with this command or declare its own justified
 * exception in annotations-contract.txt §7.
 */

import { resolve as resolvePath } from "node:path";
import type { Command } from "commander";
import {
	ANNOTATION_KIND_ORDER,
	ResolutionErrorCode,
} from "../../core/annotations/types.js";
import type { AppContext } from "../../main.js";

export function registerDocsCommand(
	program: Command,
	getCtx: () => AppContext,
): void {
	program
		.command("docs <repo> <target>")
		.description(
			"Show provisional annotations (README, package description) attached to a target",
		)
		.option("--json", "JSON output")
		.action(
			(
				repoRef: string,
				target: string,
				opts: { json?: boolean },
			) => {
				const ctx = getCtx();
				const repo =
					ctx.storage.getRepo({ uid: repoRef }) ??
					ctx.storage.getRepo({ name: repoRef }) ??
					ctx.storage.getRepo({ rootPath: resolvePath(repoRef) });
				if (!repo) {
					outputError(opts.json, `Repository not found: ${repoRef}`);
					process.exitCode = 1;
					return;
				}
				const snapshot = ctx.storage.getLatestSnapshot(repo.repoUid);
				if (!snapshot) {
					outputError(opts.json, `Repository not indexed: ${repoRef}`);
					process.exitCode = 1;
					return;
				}

				const resolution = ctx.annotations.resolveDocsTarget(
					snapshot.snapshotUid,
					target,
				);

				if (resolution.kind === "not_found") {
					if (opts.json) {
						console.log(
							JSON.stringify(
								{
									command: "docs",
									repo: repo.name,
									target_query: target,
									resolved_target: null,
									resolution_error: ResolutionErrorCode.NOT_FOUND,
									annotations: [],
									count: 0,
								},
								null,
								2,
							),
						);
					} else {
						console.log(
							`No target found for "${target}". ` +
								`Resolution tried: stable_key, path, module name. ` +
								`Use an exact stable_key, path, or module name.`,
						);
					}
					// Not an error: target resolution failure returns a
					// well-formed response with resolved_target=null.
					return;
				}

				if (resolution.kind === "ambiguous") {
					const code =
						resolution.step === "path"
							? ResolutionErrorCode.AMBIGUOUS_AT_STEP_PATH
							: ResolutionErrorCode.AMBIGUOUS_AT_STEP_MODULE_NAME;
					if (opts.json) {
						console.log(
							JSON.stringify(
								{
									command: "docs",
									repo: repo.name,
									target_query: target,
									resolved_target: null,
									resolution_error: code,
									candidates: resolution.candidates,
									annotations: [],
									count: 0,
								},
								null,
								2,
							),
						);
					} else {
						console.error(
							`Ambiguous target "${target}" at resolution step "${resolution.step}". ` +
								`Candidates:`,
						);
						for (const c of resolution.candidates) {
							console.error(`  ${c}`);
						}
						console.error(
							`Use the exact stable_key from candidates to disambiguate.`,
						);
					}
					process.exitCode = 1;
					return;
				}

				const annotations = ctx.annotations.getAnnotationsByTarget(
					snapshot.snapshotUid,
					resolution.targetStableKey,
				);

				if (opts.json) {
					console.log(
						JSON.stringify(
							{
								command: "docs",
								repo: repo.name,
								target_query: target,
								resolved_target: resolution.targetStableKey,
								annotations: annotations.map((a) => ({
									annotation_uid: a.annotation_uid,
									target_kind: a.target_kind,
									target_stable_key: a.target_stable_key,
									annotation_kind: a.annotation_kind,
									contract_class: a.contract_class,
									content: a.content,
									content_hash: a.content_hash,
									source_file: a.source_file,
									source_line_start: a.source_line_start,
									source_line_end: a.source_line_end,
									language: a.language,
									provisional: a.provisional,
									extracted_at: a.extracted_at,
								})),
								count: annotations.length,
							},
							null,
							2,
						),
					);
				} else {
					printHuman(target, resolution.targetStableKey, annotations);
				}
			},
		);
}

interface AnnotationRow {
	annotation_kind: string;
	source_file: string;
	source_line_start: number;
	content: string;
}

function printHuman(
	query: string,
	targetStableKey: string,
	annotations: AnnotationRow[],
): void {
	console.log(`Target query: ${query}`);
	console.log(`Resolved:     ${targetStableKey}`);
	console.log("");
	if (annotations.length === 0) {
		console.log("(no annotations attached to this target)");
		console.log("");
		console.log("These are provisional author claims. Use for orientation only.");
		return;
	}
	console.log(`${annotations.length} annotation(s):`);
	console.log("");
	for (const a of annotations) {
		const header = `[${a.annotation_kind}] ${a.source_file}:${a.source_line_start}`;
		console.log(header);
		console.log("-".repeat(header.length));
		// Indent content for readability
		for (const line of a.content.split("\n")) {
			console.log(`  ${line}`);
		}
		console.log("");
	}
	console.log(
		"NOTE: annotations are PROVISIONAL author claims (HINT contract). Use for orientation; do not treat as verified.",
	);
}

function outputError(json: boolean | undefined, message: string): void {
	if (json) {
		console.log(JSON.stringify({ error: message }));
	} else {
		console.error(`Error: ${message}`);
	}
}

// Re-export to silence unused import warnings when not referenced
void ANNOTATION_KIND_ORDER;
