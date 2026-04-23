/**
 * Directory-structure module detector — Layer 3B structural inference.
 *
 * Infers module boundaries from directory structure when no explicit
 * manifest or build-system artifact exists. Targets legacy monorepos,
 * pure C repos without Kbuild, and mixed-origin trees.
 *
 * Scope (B1 minimal, locked in design slice):
 *   - Immediate children of anchored roots only
 *   - Fixed anchored roots: src/, lib/, pkg/, internal/, drivers/, arch/
 *   - Fixed thresholds: file count >= 5, language coherence >= 80%
 *   - Fixed exclusions: test/vendor/third_party/etc. path segments
 *
 * NOT in scope:
 *   - Deep recursive inference (would explode into junk modules)
 *   - Adaptive thresholds
 *   - Config-based rules
 *   - Automatic boundary/forbids inference
 *
 * Pure function. No file I/O. Takes file metadata, returns discovered
 * module roots.
 *
 * See docs/milestones/module-discovery-layer-3.md for design.
 */

import type { DiscoveredModuleRoot } from "../manifest-detectors.js";

// ── Configuration ──────────────────────────────────────────────────

/**
 * Anchored root directories where module inference is permitted.
 * Only immediate children of these directories become module candidates.
 *
 * Rationale: these are conventional module-container directories across
 * ecosystems. Limiting to anchored roots prevents spurious deep-directory
 * modules.
 */
const ANCHORED_ROOTS: ReadonlySet<string> = new Set([
	"src",
	"lib",
	"pkg",
	"internal",
	"drivers",
	"arch",
]);

/**
 * Path segments that disqualify a directory from module inference.
 * Exact segment match (not substring).
 *
 * Rationale: test and vendored directories are not application modules.
 */
const EXCLUDED_SEGMENTS: ReadonlySet<string> = new Set([
	"test",
	"tests",
	"__tests__",
	"vendor",
	"vendors",
	"third_party",
	"third-party",
	"external",
	"deps",
	"node_modules",
]);

/**
 * Minimum file count under a candidate root to emit a module.
 */
const MIN_FILE_COUNT = 5;

/**
 * Minimum language coherence (0.0–1.0) to emit a module.
 * Coherence = (files with dominant language) / (total files).
 */
const MIN_LANGUAGE_COHERENCE = 0.8;

/**
 * Confidence assigned to directory-inferred modules.
 * MEDIUM band per the Layer 3 design.
 */
const DIRECTORY_CONFIDENCE = 0.7;

// ── Input types ────────────────────────────────────────────────────

/**
 * Minimal file metadata required for directory detection.
 * Compatible with TrackedFile but does not require the full type.
 */
export interface FileMetadata {
	/** Repo-relative path. */
	path: string;
	/** Detected language (e.g., "typescript", "c", "python"). */
	language: string | null;
	/** True if the file is in a test directory or has test naming. */
	isTest: boolean;
	/** True if the file is excluded from analysis. */
	isExcluded: boolean;
}

/**
 * Result of directory-structure module detection.
 */
export interface DirectoryDetectorResult {
	/** Discovered module candidates. */
	modules: DiscoveredModuleRoot[];
	/** Statistics for auditability. */
	stats: DirectoryDetectorStats;
}

export interface DirectoryDetectorStats {
	/** Total files processed. */
	filesProcessed: number;
	/** Candidate directories evaluated. */
	candidatesEvaluated: number;
	/** Candidates that passed thresholds. */
	candidatesPassed: number;
	/** Candidates rejected for insufficient files. */
	rejectedInsufficientFiles: number;
	/** Candidates rejected for low language coherence. */
	rejectedLowCoherence: number;
	/** Candidates rejected for excluded segments. */
	rejectedExcludedSegment: number;
}

// ── Detector ───────────────────────────────────────────────────────

/**
 * Detect directory-structure modules from file metadata.
 *
 * Algorithm:
 * 1. Group files by immediate child of anchored roots
 * 2. Filter out directories with excluded path segments
 * 3. Apply file count threshold
 * 4. Apply language coherence threshold
 * 5. Emit surviving directories as module candidates
 *
 * @param files - All tracked files in the repository
 * @returns Detection result with modules and statistics
 */
export function detectDirectoryModules(
	files: FileMetadata[],
): DirectoryDetectorResult {
	const stats: DirectoryDetectorStats = {
		filesProcessed: 0,
		candidatesEvaluated: 0,
		candidatesPassed: 0,
		rejectedInsufficientFiles: 0,
		rejectedLowCoherence: 0,
		rejectedExcludedSegment: 0,
	};

	// Group files by candidate root (immediate child of anchored root).
	// Key: candidate root path (e.g., "src/core", "lib/utils")
	// Value: { files: FileMetadata[], languageCounts: Map<string, number> }
	const candidates = new Map<string, {
		files: FileMetadata[];
		languageCounts: Map<string, number>;
	}>();

	for (const file of files) {
		stats.filesProcessed++;

		// Skip excluded and test files.
		if (file.isExcluded || file.isTest) continue;

		// Skip files under excluded path segments (vendor/, test/, etc.).
		// This catches nested excluded directories like src/core/vendor/lib.c
		// which should not contribute to src/core's file count.
		if (hasExcludedSegment(file.path)) continue;

		// Parse path to find candidate root.
		const candidateRoot = getCandidateRoot(file.path);
		if (!candidateRoot) continue;

		// Get or create candidate entry.
		let entry = candidates.get(candidateRoot);
		if (!entry) {
			entry = { files: [], languageCounts: new Map() };
			candidates.set(candidateRoot, entry);
		}

		entry.files.push(file);

		// Track language distribution.
		if (file.language) {
			const count = entry.languageCounts.get(file.language) || 0;
			entry.languageCounts.set(file.language, count + 1);
		}
	}

	// Evaluate each candidate.
	const modules: DiscoveredModuleRoot[] = [];

	for (const [rootPath, entry] of candidates) {
		stats.candidatesEvaluated++;

		// Check for excluded path segments in root path.
		// Note: This is largely redundant now that files are filtered by
		// hasExcludedSegment(file.path) before grouping. A candidate with
		// an excluded root can only exist if files somehow bypass the file
		// filter. Kept as a safety check.
		if (hasExcludedSegment(rootPath)) {
			stats.rejectedExcludedSegment++;
			continue;
		}

		// Check file count threshold.
		if (entry.files.length < MIN_FILE_COUNT) {
			stats.rejectedInsufficientFiles++;
			continue;
		}

		// Check language coherence.
		const coherence = computeLanguageCoherence(entry.files, entry.languageCounts);
		if (coherence < MIN_LANGUAGE_COHERENCE) {
			stats.rejectedLowCoherence++;
			continue;
		}

		// Candidate passed all thresholds.
		stats.candidatesPassed++;

		// Determine dominant language for display name hint.
		const dominantLanguage = getDominantLanguage(entry.languageCounts);

		modules.push({
			rootPath,
			displayName: rootPath.includes("/")
				? rootPath.slice(rootPath.lastIndexOf("/") + 1)
				: rootPath,
			moduleKind: "inferred",
			sourceType: "directory_structure",
			sourcePath: rootPath, // The directory itself is the "source"
			evidenceKind: "directory_pattern",
			confidence: DIRECTORY_CONFIDENCE,
			payload: {
				fileCount: entry.files.length,
				languageCoherence: Math.round(coherence * 100) / 100,
				dominantLanguage,
				anchoredRoot: rootPath.split("/")[0],
			},
		});
	}

	return { modules, stats };
}

// ── Helpers ────────────────────────────────────────────────────────

/**
 * Extract the candidate root from a file path.
 *
 * A candidate root is an immediate child of an anchored root:
 *   - "src/core/foo.ts" → "src/core"
 *   - "lib/utils/bar.py" → "lib/utils"
 *   - "drivers/net/eth.c" → "drivers/net"
 *   - "src/index.ts" → null (file directly in anchored root, not a child dir)
 *   - "other/foo.ts" → null (not under anchored root)
 *
 * @returns The candidate root path, or null if file is not under an anchored root's child.
 */
function getCandidateRoot(filePath: string): string | null {
	const segments = filePath.split("/");

	// Need at least 3 segments: anchoredRoot/childDir/file
	if (segments.length < 3) return null;

	const anchoredRoot = segments[0];
	if (!ANCHORED_ROOTS.has(anchoredRoot)) return null;

	// Candidate root is anchoredRoot + immediate child directory.
	return `${anchoredRoot}/${segments[1]}`;
}

/**
 * Check if a path contains any excluded segment.
 */
function hasExcludedSegment(path: string): boolean {
	const segments = path.split("/");
	return segments.some((seg) => EXCLUDED_SEGMENTS.has(seg));
}

/**
 * Compute language coherence: (dominant language files) / (total files).
 */
function computeLanguageCoherence(
	files: FileMetadata[],
	languageCounts: Map<string, number>,
): number {
	if (files.length === 0) return 0;

	// Find max language count.
	let maxCount = 0;
	for (const count of languageCounts.values()) {
		if (count > maxCount) maxCount = count;
	}

	// Files with null language count toward total but not toward coherence.
	// This means mixed-language-with-unknowns directories get lower coherence.
	return maxCount / files.length;
}

/**
 * Get the dominant language (most frequent).
 */
function getDominantLanguage(languageCounts: Map<string, number>): string | null {
	let maxLang: string | null = null;
	let maxCount = 0;
	for (const [lang, count] of languageCounts) {
		if (count > maxCount) {
			maxCount = count;
			maxLang = lang;
		}
	}
	return maxLang;
}
