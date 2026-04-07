/**
 * package.json script CLI consumer extractor.
 *
 * Parses package.json "scripts" entries and emits BoundaryConsumerFact
 * records with mechanism `cli_command` for each direct tool invocation.
 *
 * Uses the shared cli-invocation-parser support module for command
 * parsing, chain splitting, wrapper unwrapping, and builtin filtering.
 *
 * Maturity: PROTOTYPE.
 */

import type { BoundaryConsumerFact } from "../../../core/classification/api-boundary.js";
import {
	processCommandSegment,
	splitShellChain,
} from "../cli/cli-invocation-parser.js";

/**
 * Extract CLI consumer facts from package.json scripts.
 *
 * @param scripts - The "scripts" object from package.json.
 * @param filePath - Repo-relative path to the package.json file.
 * @param repoUid - Repository UID.
 */
export function extractPackageScriptConsumers(
	scripts: Record<string, string>,
	filePath: string,
	_repoUid: string,
): BoundaryConsumerFact[] {
	const facts: BoundaryConsumerFact[] = [];

	for (const [scriptName, scriptBody] of Object.entries(scripts)) {
		if (!scriptBody || typeof scriptBody !== "string") continue;

		const segments = splitShellChain(scriptBody);

		for (const segment of segments) {
			const parsed = processCommandSegment(segment);
			if (!parsed) continue;

			facts.push({
				mechanism: "cli_command",
				operation: `CLI ${parsed.commandPath}`,
				address: parsed.commandPath,
				callerStableKey: `${filePath}:scripts.${scriptName}`,
				sourceFile: filePath,
				lineStart: 0,
				basis: "literal",
				confidence: parsed.confidence,
				schemaRef: null,
				metadata: {
					scriptName,
					rawCommand: segment.trim().slice(0, 100),
					binary: parsed.binary,
					args: parsed.args,
				},
			});
		}
	}

	return facts;
}
