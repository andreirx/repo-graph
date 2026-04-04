/**
 * Coverage importer registry.
 *
 * Holds all registered coverage format adapters and provides
 * auto-detection: given a report file, tries each importer
 * until one claims it can handle the format.
 */

import { readFile } from "node:fs/promises";
import type { CoverageImporterPort } from "../../core/ports/coverage-importer.js";
import { IstanbulCoverageImporter } from "./istanbul-coverage.js";

/** All registered coverage importers, in detection priority order. */
const IMPORTERS: CoverageImporterPort[] = [
	new IstanbulCoverageImporter(),
	// Future: new JacocoCoverageImporter(),
	// Future: new LcovCoverageImporter(),
	// Future: new CoberturaCoverageImporter(),
];

/**
 * Detect the coverage format and return the matching importer.
 *
 * Reads the first 4KB of the file for format sniffing, then tries
 * each registered importer's canHandle() method.
 *
 * @returns The matching importer, or null if no format matched.
 */
export async function detectCoverageImporter(
	reportPath: string,
): Promise<CoverageImporterPort | null> {
	let content: string;
	try {
		const fullContent = await readFile(reportPath, "utf-8");
		content = fullContent.slice(0, 4096);
	} catch {
		return null;
	}

	for (const importer of IMPORTERS) {
		if (importer.canHandle(reportPath, content)) {
			return importer;
		}
	}
	return null;
}

/**
 * Get all registered format names (for error messages).
 */
export function supportedFormats(): string[] {
	return IMPORTERS.map((i) => i.formatName);
}
