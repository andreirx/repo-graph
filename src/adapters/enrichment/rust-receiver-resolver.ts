/**
 * Rust receiver type binding via rust-analyzer LSP.
 *
 * Parallel to typescript-receiver-resolver.ts: a post-index
 * enrichment side-channel that resolves receiver types on unresolved
 * Rust obj.method() calls. Uses rust-analyzer as an LSP subprocess.
 *
 * Architecture:
 *   - Starts rust-analyzer as a child process
 *   - Communicates via JSON-RPC (LSP protocol)
 *   - Sends textDocument/hover at each call site's receiver position
 *   - Parses the type from the hover response
 *   - Returns the same ReceiverTypeResult shape as the TS resolver
 *
 * Requirements:
 *   - rust-analyzer on PATH (installed via `rustup component add rust-analyzer`)
 *   - Cargo.toml in the project root (rust-analyzer needs it)
 *
 * Performance:
 *   - rust-analyzer startup + project loading: 5-30 seconds
 *   - per-query: milliseconds (after loading)
 *   - designed for the `rgr enrich` post-index pass
 */

import { spawn, type ChildProcess } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import type { ReceiverTypeResult, EnrichmentProgress } from "./typescript-receiver-resolver.js";

// ── LSP message helpers ─────────────────────────────────────────────

let msgId = 1;

function lspRequest(method: string, params: unknown): string {
	const body = JSON.stringify({ jsonrpc: "2.0", id: msgId++, method, params });
	return `Content-Length: ${Buffer.byteLength(body)}\r\n\r\n${body}`;
}

function lspNotification(method: string, params: unknown): string {
	const body = JSON.stringify({ jsonrpc: "2.0", method, params });
	return `Content-Length: ${Buffer.byteLength(body)}\r\n\r\n${body}`;
}

interface LspResponse {
	id: number;
	result?: unknown;
	error?: { code: number; message: string };
}

// ── Rust-analyzer subprocess ────────────────────────────────────────

class RustAnalyzerClient {
	private proc: ChildProcess | null = null;
	private buffer = "";
	private pendingResolves = new Map<number, (response: LspResponse) => void>();

	async start(rootPath: string): Promise<boolean> {
		return new Promise((resolve) => {
			try {
				this.proc = spawn("rust-analyzer", [], {
					stdio: ["pipe", "pipe", "pipe"],
					cwd: rootPath,
				});
			} catch {
				resolve(false);
				return;
			}

			if (!this.proc.stdout || !this.proc.stdin) {
				resolve(false);
				return;
			}

			this.proc.stdout.on("data", (chunk: Buffer) => {
				this.buffer += chunk.toString();
				this.processBuffer();
			});

			this.proc.on("error", () => resolve(false));
			this.proc.on("exit", () => {
				this.proc = null;
			});

			// Send initialize request.
			const initId = msgId;
			this.proc.stdin.write(
				lspRequest("initialize", {
					processId: process.pid,
					rootUri: `file://${encodeURI(rootPath).replace(/%3A/g, ":")}`,
					capabilities: {},
					initializationOptions: {
						// Disable features we don't need for faster startup.
						checkOnSave: { enable: false },
						diagnostics: { enable: false },
						inlayHints: { enable: false },
						lens: { enable: false },
					},
				}),
			);

			// Wait for initialize response.
			this.waitForResponse(initId, 60000).then((resp) => {
				if (resp?.result) {
					// Send initialized notification.
					this.proc?.stdin?.write(
						lspNotification("initialized", {}),
					);
					resolve(true);
				} else {
					resolve(false);
				}
			}).catch(() => resolve(false));
		});
	}

	private openedFiles = new Set<string>();

	async hover(
		filePath: string,
		line: number,
		col: number,
	): Promise<string | null> {
		if (!this.proc?.stdin) return null;

		const uri = `file://${encodeURI(filePath).replace(/%3A/g, ":")}`;

		const hoverId = msgId;

		// Open the document (LSP requires didOpen before hover).
		if (!this.openedFiles.has(uri)) {
			let text = "";
			try {
				text = readFileSync(filePath, "utf-8");
			} catch {
				return null;
			}
			this.proc.stdin.write(
				lspNotification("textDocument/didOpen", {
					textDocument: {
						uri,
						languageId: "rust",
						version: 1,
						text,
					},
				}),
			);
			this.openedFiles.add(uri);
		}

		// Send hover request.
		this.proc.stdin.write(
			lspRequest("textDocument/hover", {
				textDocument: { uri },
				position: { line: line - 1, character: col },
			}),
		);

		const resp = await this.waitForResponse(hoverId, 10000);
		if (!resp?.result) return null;

		const result = resp.result as {
			contents?: { kind?: string; value?: string } | string;
		};
		const contents = result.contents;
		if (!contents) return null;

		// Extract type from hover markdown.
		const text = typeof contents === "string"
			? contents
			: (contents.value ?? "");
		return extractTypeFromHover(text);
	}

	async stop(): Promise<void> {
		if (!this.proc?.stdin) return;
		const shutdownId = msgId;
		this.proc.stdin.write(lspRequest("shutdown", null));
		await this.waitForResponse(shutdownId, 5000).catch(() => {});
		this.proc.stdin.write(lspNotification("exit", null));
		this.proc.kill();
		this.proc = null;
	}

	private processBuffer(): void {
		while (true) {
			const headerEnd = this.buffer.indexOf("\r\n\r\n");
			if (headerEnd < 0) break;
			const header = this.buffer.slice(0, headerEnd);
			const match = header.match(/Content-Length:\s*(\d+)/i);
			if (!match) {
				this.buffer = this.buffer.slice(headerEnd + 4);
				continue;
			}
			const bodyLen = Number.parseInt(match[1], 10);
			const bodyStart = headerEnd + 4;
			if (this.buffer.length < bodyStart + bodyLen) break;
			const body = this.buffer.slice(bodyStart, bodyStart + bodyLen);
			this.buffer = this.buffer.slice(bodyStart + bodyLen);

			try {
				const msg = JSON.parse(body) as LspResponse;
				if (msg.id !== undefined) {
					const resolve = this.pendingResolves.get(msg.id);
					if (resolve) {
						this.pendingResolves.delete(msg.id);
						resolve(msg);
					}
				}
			} catch {
				// ignore malformed
			}
		}
	}

	private waitForResponse(
		id: number,
		timeoutMs: number,
	): Promise<LspResponse | null> {
		return new Promise((resolve) => {
			const timer = setTimeout(() => {
				this.pendingResolves.delete(id);
				resolve(null);
			}, timeoutMs);
			this.pendingResolves.set(id, (resp) => {
				clearTimeout(timer);
				resolve(resp);
			});
		});
	}
}

// ── Type extraction from hover ──────────────────────────────────────

/** Exported for testing. */
export function extractTypeFromHover(hoverText: string): string | null {
	// rust-analyzer hover returns markdown with ```rust blocks.
	// Strip the markdown fences to get raw Rust text.
	const stripped = hoverText
		.replace(/```rust\n?/g, "")
		.replace(/```\n?/g, "")
		.trim();

	if (!stripped) return null;

	// Pattern 1: type annotation — "name: Type" or "let name: Type"
	// Captures the type after the colon. Handles qualified paths
	// (crate::engine::EngineContext, std::collections::HashMap) and
	// simple types (Vec, u32). Takes the LAST path segment as the
	// type name when a qualified path is present.
	const typeAnnotation = stripped.match(
		/(?:^|\s)\w+\s*:\s*(&?\s*(?:mut\s+)?[A-Za-z][\w]*(?:::[A-Za-z][\w]*)*(?:<[^>]*>)?)/m,
	);
	if (typeAnnotation) {
		return cleanTypeName(typeAnnotation[1]);
	}

	// Pattern 2: struct/enum/trait/type definition header.
	const defMatch = stripped.match(
		/(?:pub\s+)?(?:struct|enum|trait|type)\s+([A-Z][\w]*)/,
	);
	if (defMatch) return defMatch[1];

	// Pattern 3: reference type "&Type" or "&mut Type" (hover on self).
	const refMatch = stripped.match(/^&(?:mut\s+)?([A-Z][\w]*)/);
	if (refMatch) return refMatch[1];

	// Pattern 4: plain type name starting with uppercase (PascalCase).
	const plainMatch = stripped.match(/^([A-Z][\w]*)/);
	if (plainMatch) return plainMatch[1];

	return null;
}

/**
 * Clean a raw type string from hover text:
 * - Strip leading &/&mut
 * - Strip generic parameters for the display name
 * - Strip lifetime parameters
 */
function cleanTypeName(raw: string): string | null {
	let name = raw.trim();
	// Strip reference markers
	name = name.replace(/^&\s*(?:mut\s+)?/, "");
	// Strip lifetimes
	name = name.replace(/<'[^>]*>/g, "");
	// Take base type before generics
	const angleIdx = name.indexOf("<");
	if (angleIdx > 0) name = name.slice(0, angleIdx).trim();
	// For qualified paths (crate::engine::EngineContext), take the
	// last segment as the type name.
	if (name.includes("::")) {
		const segments = name.split("::");
		name = segments[segments.length - 1].trim();
	}
	return name || null;
}

/**
 * Validate that a string is a plausible Rust type name.
 * Rejects keywords, single-letter identifiers, variable names,
 * and other hover artifacts that are NOT type names.
 */
/** Exported for testing. */
export function isValidRustTypeName(name: string): boolean {
	if (!name || name.length < 2) return false;
	// Reject Rust keywords and common non-type tokens.
	const REJECT = new Set([
		"self", "Self", "let", "mut", "fn", "pub", "mod", "use",
		"impl", "trait", "struct", "enum", "type", "const", "static",
		"return", "if", "else", "match", "for", "while", "loop",
		"break", "continue", "async", "await", "move", "ref",
		"where", "as", "in", "true", "false", "crate", "super",
		"any", "unknown", "{unknown}", "test", "def",
	]);
	if (REJECT.has(name)) return false;
	// Reject names that contain newlines (hover markdown leaks).
	if (name.includes("\n")) return false;
	// Must start with an uppercase letter (Rust type convention)
	// OR be a known primitive type.
	const PRIMITIVES = new Set([
		"bool", "char", "str",
		"i8", "i16", "i32", "i64", "i128", "isize",
		"u8", "u16", "u32", "u64", "u128", "usize",
		"f32", "f64",
	]);
	if (PRIMITIVES.has(name)) return true;
	if (/^[A-Z]/.test(name)) return true;
	return false;
}

// ── Public API ──────────────────────────────────────────────────────

/**
 * Resolve receiver types for Rust call sites using rust-analyzer.
 */
/**
 * Resolve receiver types for Rust call sites using rust-analyzer.
 *
 * Groups sites by nearest-owning Cargo.toml and starts one
 * rust-analyzer instance per Cargo context. This handles:
 *   - repos where Cargo.toml is not at the repo root
 *   - repos with multiple independent Cargo projects
 *   - workspace repos (workspace root's Cargo.toml serves all crates)
 */
export async function resolveRustReceiverTypes(
	repoRootPath: string,
	sites: ReadonlyArray<{
		edgeUid: string;
		sourceFilePath: string;
		lineStart: number;
		colStart: number;
		targetKey: string;
	}>,
	onProgress?: (p: EnrichmentProgress) => void,
): Promise<ReceiverTypeResult[]> {
	if (sites.length === 0) return [];
	const emit = onProgress ?? (() => {});

	// Group sites by nearest Cargo.toml ancestor directory.
	const groups = groupSitesByCargoRoot(repoRootPath, sites);

	const allResults: ReceiverTypeResult[] = [];
	let totalProcessed = 0;

	for (const [cargoRoot, groupSites] of groups) {
		emit({ phase: "starting_rust_analyzer", current: totalProcessed, total: sites.length });

		const client = new RustAnalyzerClient();
		const started = await client.start(cargoRoot);
		if (!started) {
			for (const s of groupSites) {
				allResults.push({
					edgeUid: s.edgeUid,
					receiverType: null,
					receiverTypeOrigin: "failed" as const,
					typeDisplayName: null,
					isExternalType: false,
					failureReason: `rust-analyzer failed to start at ${cargoRoot}`,
				});
			}
			totalProcessed += groupSites.length;
			continue;
		}

		// Warm up: retry hover on the first site until rust-analyzer loads.
		emit({ phase: "loading_project", current: totalProcessed, total: sites.length });
		const firstAbsPath = join(repoRootPath, groupSites[0].sourceFilePath);
		let warmedUp = false;
		for (let attempt = 0; attempt < 30; attempt++) {
			const warmup = await client.hover(
				firstAbsPath,
				groupSites[0].lineStart,
				groupSites[0].colStart,
			);
			if (warmup !== null) {
				warmedUp = true;
				break;
			}
			await new Promise((r) => setTimeout(r, 2000));
		}

		if (!warmedUp) {
			await client.stop();
			for (const s of groupSites) {
				allResults.push({
					edgeUid: s.edgeUid,
					receiverType: null,
					receiverTypeOrigin: "failed" as const,
					typeDisplayName: null,
					isExternalType: false,
					failureReason: "rust-analyzer did not respond after loading timeout",
				});
			}
			totalProcessed += groupSites.length;
			continue;
		}

		// Resolve types for this group.
		for (const site of groupSites) {
			emit({ phase: "resolving_types", current: totalProcessed, total: sites.length });

			const absPath = join(repoRootPath, site.sourceFilePath);
			const typeName = await client.hover(absPath, site.lineStart, site.colStart);

			if (typeName && isValidRustTypeName(typeName)) {
				allResults.push({
					edgeUid: site.edgeUid,
					receiverType: typeName,
					receiverTypeOrigin: "compiler",
					typeDisplayName: typeName,
					isExternalType: isRustExternalType(typeName),
					failureReason: null,
				});
			} else {
				allResults.push({
					edgeUid: site.edgeUid,
					receiverType: null,
					receiverTypeOrigin: "failed",
					typeDisplayName: null,
					isExternalType: false,
					failureReason: typeName
						? `type is ${typeName}`
						: "hover returned no type",
				});
			}

			totalProcessed++;
		}

		await client.stop();
	}

	emit({ phase: "done", current: sites.length, total: sites.length });
	return allResults;
}

// ── Cargo root grouping ─────────────────────────────────────────────

type Site = {
	edgeUid: string;
	sourceFilePath: string;
	lineStart: number;
	colStart: number;
	targetKey: string;
};

/**
 * Group sites by their nearest Cargo.toml ancestor directory.
 * Walks upward from each file's directory to the repo root.
 * Falls back to repoRootPath if no Cargo.toml is found.
 */
function groupSitesByCargoRoot(
	repoRootPath: string,
	sites: ReadonlyArray<Site>,
): Map<string, Site[]> {
	const cache = new Map<string, string>();
	const groups = new Map<string, Site[]>();

	for (const site of sites) {
		const cargoRoot = findNearestCargoRoot(
			site.sourceFilePath,
			repoRootPath,
			cache,
		);
		const group = groups.get(cargoRoot) ?? [];
		group.push(site);
		groups.set(cargoRoot, group);
	}

	return groups;
}

function findNearestCargoRoot(
	fileRelPath: string,
	repoRoot: string,
	cache: Map<string, string>,
): string {
	let dir = fileRelPath.includes("/")
		? fileRelPath.slice(0, fileRelPath.lastIndexOf("/"))
		: "";

	// Walk upward checking cache + filesystem.
	const uncached: string[] = [];
	while (true) {
		const cached = cache.get(dir);
		if (cached !== undefined) {
			for (const d of uncached) cache.set(d, cached);
			return cached;
		}
		uncached.push(dir);

		const absDir = dir === "" ? repoRoot : join(repoRoot, dir);
		if (existsSync(join(absDir, "Cargo.toml"))) {
			for (const d of uncached) cache.set(d, absDir);
			return absDir;
		}

		if (dir === "") break;
		const slash = dir.lastIndexOf("/");
		dir = slash >= 0 ? dir.slice(0, slash) : "";
	}

	// No Cargo.toml found — fall back to repo root.
	for (const d of uncached) cache.set(d, repoRoot);
	return repoRoot;
}

function isRustExternalType(typeName: string): boolean {
	// Rust std types are external (runtime).
	const stdTypes = new Set([
		"Vec", "String", "HashMap", "HashSet", "BTreeMap", "BTreeSet",
		"VecDeque", "LinkedList", "BinaryHeap",
		"Box", "Rc", "Arc", "Cell", "RefCell", "Mutex", "RwLock",
		"Option", "Result", "Cow",
		"Path", "PathBuf", "OsString", "OsStr",
		"File", "BufReader", "BufWriter",
		"TcpStream", "TcpListener", "UdpSocket",
		"Duration", "Instant", "SystemTime",
		"Command", "Child",
		"Error",
	]);
	return stdTypes.has(typeName);
}
