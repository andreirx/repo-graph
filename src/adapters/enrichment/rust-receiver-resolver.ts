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
import { readFileSync } from "node:fs";
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
					rootUri: `file://${rootPath}`,
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

		const uri = `file://${filePath}`;
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

	emit({ phase: "starting_rust_analyzer", current: 0, total: sites.length });

	const client = new RustAnalyzerClient();
	const started = await client.start(repoRootPath);
	if (!started) {
		// rust-analyzer unavailable — return all as failed.
		return sites.map((s) => ({
			edgeUid: s.edgeUid,
			receiverType: null,
			receiverTypeOrigin: "failed" as const,
			typeDisplayName: null,
			isExternalType: false,
			failureReason: "rust-analyzer not available or failed to start",
		}));
	}

	// Wait for rust-analyzer to finish loading the project.
	// Send a warm-up hover on the first file and retry until we
	// get a non-null response. rust-analyzer queues requests but
	// may return empty results before indexing completes.
	emit({ phase: "loading_project", current: 0, total: sites.length });
	const firstAbsPath = join(repoRootPath, sites[0].sourceFilePath);
	let warmedUp = false;
	for (let attempt = 0; attempt < 30; attempt++) {
		const warmup = await client.hover(firstAbsPath, sites[0].lineStart, sites[0].colStart);
		if (warmup !== null) {
			warmedUp = true;
			break;
		}
		// Wait 2 seconds between retries while rust-analyzer loads.
		await new Promise((r) => setTimeout(r, 2000));
	}
	if (!warmedUp) {
		// Could not get a response after 60 seconds of waiting.
		// rust-analyzer may not support this project or is too slow.
		await client.stop();
		return sites.map((s) => ({
			edgeUid: s.edgeUid,
			receiverType: null,
			receiverTypeOrigin: "failed" as const,
			typeDisplayName: null,
			isExternalType: false,
			failureReason: "rust-analyzer did not respond after loading timeout",
		}));
	}

	const results: ReceiverTypeResult[] = [];
	let processed = 0;

	for (const site of sites) {
		emit({ phase: "resolving_types", current: processed, total: sites.length });

		const absPath = join(repoRootPath, site.sourceFilePath);

		// For obj.method() or self.field.method(), hover on the receiver.
		// The receiver is the first segment of the targetKey.
		const receiverCol = site.colStart;

		const typeName = await client.hover(absPath, site.lineStart, receiverCol);

		if (typeName && isValidRustTypeName(typeName)) {
			// Heuristic: types from std/core/alloc are "external" (stdlib).
			// Types matching the project's own modules are "internal".
			const isExternal = isRustExternalType(typeName);

			results.push({
				edgeUid: site.edgeUid,
				receiverType: typeName,
				receiverTypeOrigin: "compiler",
				typeDisplayName: typeName,
				isExternalType: isExternal,
				failureReason: null,
			});
		} else {
			results.push({
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

		processed++;
	}

	emit({ phase: "stopping", current: sites.length, total: sites.length });
	await client.stop();
	emit({ phase: "done", current: sites.length, total: sites.length });

	return results;
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
