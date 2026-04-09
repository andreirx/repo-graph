/**
 * Topology enrichment — derives config root and entrypoint links
 * from project surfaces and their evidence.
 *
 * Pure function. No filesystem, no storage.
 *
 * Takes persisted project surfaces + evidence and produces
 * SurfaceConfigRoot[] and SurfaceEntrypoint[] for persistence.
 *
 * Config roots are derived from:
 *   - The manifest file that produced the surface evidence
 *     (package.json, Cargo.toml, pyproject.toml)
 *   - Known companion config files at the same root
 *     (tsconfig.json for TS surfaces, compile_commands.json for C/C++)
 *
 * Entrypoints are derived from:
 *   - Surface entrypointPath (already on ProjectSurface)
 *   - Surface evidence payload (bin targets, scripts, etc.)
 */

import { v4 as uuidv4 } from "uuid";
import type { ProjectSurface, ProjectSurfaceEvidence } from "../runtime/project-surface.js";
import type { ConfigRootKind, SurfaceConfigRoot, SurfaceEntrypoint, EntrypointKind } from "./topology-links.js";

// ── Input/Output ───────────────────────────────────────────────────

export interface TopologyEnrichmentInput {
	repoUid: string;
	snapshotUid: string;
	surfaces: ProjectSurface[];
	evidence: ProjectSurfaceEvidence[];
	/**
	 * Pre-resolved companion config paths per surface root.
	 * Map: rootPath → array of { path, kind, confidence }.
	 * Computed by the adapter layer (filesystem probing), not core.
	 */
	companionConfigs: Map<string, Array<{ path: string; kind: ConfigRootKind; confidence: number }>>;
}

export interface TopologyEnrichmentResult {
	configRoots: SurfaceConfigRoot[];
	entrypoints: SurfaceEntrypoint[];
}

// ── Enrichment ─────────────────────────────────────────────────────

export function enrichTopology(input: TopologyEnrichmentInput): TopologyEnrichmentResult {
	const { repoUid, snapshotUid, surfaces, evidence, companionConfigs } = input;

	const configRoots: SurfaceConfigRoot[] = [];
	const entrypoints: SurfaceEntrypoint[] = [];

	// Group evidence by surface UID.
	const evidenceBySurface = new Map<string, ProjectSurfaceEvidence[]>();
	for (const e of evidence) {
		const list = evidenceBySurface.get(e.projectSurfaceUid) ?? [];
		list.push(e);
		evidenceBySurface.set(e.projectSurfaceUid, list);
	}

	for (const surface of surfaces) {
		const surfaceEvidence = evidenceBySurface.get(surface.projectSurfaceUid) ?? [];

		// 1. Config roots from evidence source paths.
		const seenConfigs = new Set<string>();
		for (const ev of surfaceEvidence) {
			const configKind = mapSourceToConfigKind(ev.sourceType);
			if (configKind && !seenConfigs.has(ev.sourcePath)) {
				seenConfigs.add(ev.sourcePath);
				configRoots.push({
					surfaceConfigRootUid: uuidv4(),
					projectSurfaceUid: surface.projectSurfaceUid,
					snapshotUid,
					repoUid,
					configPath: ev.sourcePath,
					configKind,
					confidence: ev.confidence,
					metadataJson: null,
				});
			}
		}

		// 2. Companion config files pre-resolved by the adapter layer.
		const companions = companionConfigs.get(surface.rootPath) ?? [];
		for (const comp of companions) {
			if (!seenConfigs.has(comp.path)) {
				seenConfigs.add(comp.path);
				configRoots.push({
					surfaceConfigRootUid: uuidv4(),
					projectSurfaceUid: surface.projectSurfaceUid,
					snapshotUid,
					repoUid,
					configPath: comp.path,
					configKind: comp.kind,
					confidence: comp.confidence,
					metadataJson: null,
				});
			}
		}

		// 3. Entrypoints from surface + evidence.
		if (surface.entrypointPath) {
			const kind = mapSurfaceKindToEntrypointKind(surface.surfaceKind);
			entrypoints.push({
				surfaceEntrypointUid: uuidv4(),
				projectSurfaceUid: surface.projectSurfaceUid,
				snapshotUid,
				repoUid,
				entrypointPath: surface.entrypointPath,
				entrypointTarget: null,
				entrypointKind: kind,
				displayName: surface.displayName,
				confidence: surface.confidence,
				metadataJson: null,
			});
		}

		// Additional entrypoints from evidence payloads.
		for (const ev of surfaceEvidence) {
			if (!ev.payloadJson) continue;
			const payload = JSON.parse(ev.payloadJson) as Record<string, unknown>;

			// Python console_scripts have module:function targets.
			if (ev.sourceType === "pyproject_scripts" && typeof payload.target === "string") {
				entrypoints.push({
					surfaceEntrypointUid: uuidv4(),
					projectSurfaceUid: surface.projectSurfaceUid,
					snapshotUid,
					repoUid,
					entrypointPath: null,
					entrypointTarget: payload.target as string,
					entrypointKind: "script",
					displayName: typeof payload.scriptName === "string" ? payload.scriptName as string : null,
					confidence: ev.confidence,
					metadataJson: null,
				});
			}
		}
	}

	return { configRoots, entrypoints };
}

// ── Helpers ────────────────────────────────────────────────────────

function mapSourceToConfigKind(sourceType: string): ConfigRootKind | null {
	switch (sourceType) {
		case "package_json_bin":
		case "package_json_scripts":
		case "package_json_main":
		case "package_json_deps":
			return "package_json";
		case "cargo_bin_target":
		case "cargo_lib_target":
			return "cargo_toml";
		case "pyproject_scripts":
		case "pyproject_entry_points":
			return "pyproject_toml";
		case "gradle_application_plugin":
			return "build_gradle";
		case "dockerfile":
			return "dockerfile";
		case "docker_compose":
			return "docker_compose";
		case "compile_commands":
			return "compile_commands_json";
		case "framework_dependency":
			return "package_json"; // Framework deps come from package.json
		default:
			return null;
	}
}

function mapSurfaceKindToEntrypointKind(surfaceKind: string): EntrypointKind {
	switch (surfaceKind) {
		case "cli": return "binary";
		case "library": return "main_module";
		case "backend_service": return "server_start";
		case "worker": return "handler";
		case "web_app": return "main_module";
		case "test_harness": return "test_entry";
		default: return "main_module";
	}
}

