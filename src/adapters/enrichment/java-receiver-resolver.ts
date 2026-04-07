/**
 * Java receiver type binding via Eclipse JDT Language Server (jdtls).
 *
 * Same pattern as rust-receiver-resolver.ts: post-index enrichment
 * side-channel using an LSP subprocess for receiver type resolution.
 *
 * Architecture:
 *   - Starts jdtls as a child process
 *   - Communicates via JSON-RPC (LSP protocol)
 *   - Sends textDocument/hover at each call site's receiver position
 *   - Parses the type from the hover response
 *   - Groups sites by nearest build.gradle/pom.xml project root
 *
 * Requirements:
 *   - `jdtls` on PATH (installed via brew, sdkman, or manual download)
 *   - JDK installed
 *   - A Gradle or Maven project for jdtls to analyze
 */

import { spawn, type ChildProcess } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import type { ReceiverTypeResult, EnrichmentProgress } from "./typescript-receiver-resolver.js";

// ── LSP message helpers (shared with Rust resolver) ─────────────────

let msgId = 10000; // Offset from Rust resolver's msgId space.

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

// ── JDT Language Server client ──────────────────────────────────────

class JdtlsClient {
	private proc: ChildProcess | null = null;
	private buffer = "";
	private pendingResolves = new Map<number, (response: LspResponse) => void>();
	private openedFiles = new Set<string>();
	private dataDir: string;
	/** Server methods seen during startup — logged for diagnostics. */
	serverRequestsSeen: string[] = [];
	/** Stderr lines captured from jdtls — for diagnostics. */
	stderrLines: string[] = [];
	/** True when jdtls has signaled that project import is done. */
	private projectReady = false;

	constructor(projectRoot: string) {
		const hash = createHash("md5").update(projectRoot).digest("hex").slice(0, 8);
		this.dataDir = join(tmpdir(), `rgr-jdtls-${hash}`);
		mkdirSync(this.dataDir, { recursive: true });
	}

	/** Reason for last startup failure, if any. */
	startupFailureReason = "";

	async start(projectRoot: string): Promise<boolean> {
		return new Promise((resolve) => {
			try {
				this.proc = spawn("jdtls", [
					"-data", this.dataDir,
				], {
					stdio: ["pipe", "pipe", "pipe"],
					cwd: projectRoot,
				});
			} catch (err) {
				this.startupFailureReason = `spawn_throw: ${err instanceof Error ? err.message : String(err)}`;
				resolve(false);
				return;
			}

			if (!this.proc.stdout || !this.proc.stdin) {
				this.startupFailureReason = "missing_stdio";
				resolve(false);
				return;
			}

			let settled = false;

			this.proc.stdout.on("data", (chunk: Buffer) => {
				this.buffer += chunk.toString();
				this.processBuffer();
			});

			this.proc.stderr?.on("data", (chunk: Buffer) => {
				const lines = chunk.toString().split("\n").filter((l) => l.trim());
				this.stderrLines.push(...lines.slice(0, 50));
				if (this.stderrLines.length > 200) {
					this.stderrLines.splice(0, this.stderrLines.length - 200);
				}
			});

			this.proc.on("error", (err) => {
				if (!settled) {
					settled = true;
					this.startupFailureReason = `proc_error: ${err.message}`;
					resolve(false);
				}
			});

			this.proc.on("exit", (code, signal) => {
				if (!settled) {
					settled = true;
					this.startupFailureReason = `proc_exit_before_init: code=${code} signal=${signal}`;
					resolve(false);
				}
				this.proc = null;
			});

			const initId = msgId;
			const rootUri = `file://${encodeURI(projectRoot).replace(/%3A/g, ":")}`;

			// Register the response handler BEFORE sending the request.
			const initPromise = this.waitForResponse(initId, 120000);

			this.proc.stdin.write(
				lspRequest("initialize", {
					processId: process.pid,
					rootUri,
					capabilities: {},
					initializationOptions: {},
				}),
			);

			initPromise.then((resp) => {
				if (settled) return;
				settled = true;
				if (resp?.result) {
					this.proc?.stdin?.write(lspNotification("initialized", {}));
					resolve(true);
				} else if (resp?.error) {
					this.startupFailureReason = `init_error_response: ${resp.error.message}`;
					resolve(false);
				} else {
					this.startupFailureReason = `init_timeout: no response in 120s. stderr_tail=[${this.stderrLines.slice(-3).join(" | ")}] serverReqs=[${this.serverRequestsSeen.join(",")}]`;
					resolve(false);
				}
			}).catch((err) => {
				if (!settled) {
					settled = true;
					this.startupFailureReason = `init_promise_rejected: ${err}`;
					resolve(false);
				}
			});
		});
	}

	async hover(filePath: string, line: number, col: number): Promise<string | null> {
		if (!this.proc?.stdin) return null;

		const uri = `file://${encodeURI(filePath).replace(/%3A/g, ":")}`;
		const hoverId = msgId;

		if (!this.openedFiles.has(uri)) {
			let text = "";
			try { text = readFileSync(filePath, "utf-8"); } catch { return null; }
			this.proc.stdin.write(
				lspNotification("textDocument/didOpen", {
					textDocument: { uri, languageId: "java", version: 1, text },
				}),
			);
			this.openedFiles.add(uri);
		}

		this.proc.stdin.write(
			lspRequest("textDocument/hover", {
				textDocument: { uri },
				position: { line: line - 1, character: col },
			}),
		);

		const resp = await this.waitForResponse(hoverId, 15000);
		if (!resp?.result) return null;

		const result = resp.result as {
			contents?:
				| string
				| { kind?: string; value?: string }
				| Array<string | { language?: string; value?: string }>;
		};
		const contents = result.contents;
		if (!contents) return null;

		// jdtls returns contents in multiple possible shapes:
		// 1. string (simple text)
		// 2. {kind: "markdown", value: "..."}
		// 3. [{language: "java", value: "..."}, "description..."]
		let text: string;
		if (typeof contents === "string") {
			text = contents;
		} else if (Array.isArray(contents)) {
			// Take the first code block (language: "java").
			const codeBlock = contents.find(
				(c): c is { language: string; value: string } =>
					typeof c === "object" && c !== null && "language" in c,
			);
			text = codeBlock?.value ?? "";
		} else {
			text = contents.value ?? "";
		}

		if (!text) return null;
		return extractJavaTypeFromHover(text);
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
			if (!match) { this.buffer = this.buffer.slice(headerEnd + 4); continue; }
			const bodyLen = Number.parseInt(match[1], 10);
			const bodyStart = headerEnd + 4;
			if (this.buffer.length < bodyStart + bodyLen) break;
			const body = this.buffer.slice(bodyStart, bodyStart + bodyLen);
			this.buffer = this.buffer.slice(bodyStart + bodyLen);
			try {
				const msg = JSON.parse(body) as Record<string, unknown>;

				if (msg.id !== undefined && msg.method !== undefined) {
					// Server→client REQUEST — MUST respond.
					this.handleServerRequest(
						msg.id as number,
						msg.method as string,
						msg.params,
					);
				} else if (msg.id !== undefined) {
					// Response to our request.
					const resolve = this.pendingResolves.get(msg.id as number);
					if (resolve) {
						this.pendingResolves.delete(msg.id as number);
						resolve(msg as unknown as LspResponse);
					}
				} else if (msg.method !== undefined) {
					// Server notification.
					this.handleServerNotification(
						msg.method as string,
						msg.params,
					);
				}
			} catch { /* ignore malformed */ }
		}
	}

	/**
	 * Handle server→client LSP requests. jdtls sends these during
	 * project import and BLOCKS until it gets a response. Without
	 * handling them, the warm-up hover never resolves.
	 */
	private handleServerRequest(
		id: number,
		method: string,
		params: unknown,
	): void {
		if (!this.proc?.stdin) return;
		this.serverRequestsSeen.push(method);

		let result: unknown = null;

		switch (method) {
			case "workspace/configuration": {
				// Return one empty config per requested item.
				// jdtls sends { items: [{section: "java"}, ...] }.
				const items = (params as { items?: unknown[] })?.items;
				const count = Array.isArray(items) ? items.length : 1;
				result = Array.from({ length: count }, () => ({}));
				break;
			}
			case "client/registerCapability":
				result = null;
				break;
			case "window/workDoneProgress/create":
				result = null;
				break;
			default:
				// Unknown server request — respond with null to unblock.
				result = null;
				break;
		}

		const response = JSON.stringify({
			jsonrpc: "2.0",
			id,
			result,
		});
		this.proc.stdin.write(
			`Content-Length: ${Buffer.byteLength(response)}\r\n\r\n${response}`,
		);
	}

	/**
	 * Handle server notifications. Tracks work-done progress to
	 * detect when project import completes.
	 */
	private handleServerNotification(
		method: string,
		params: unknown,
	): void {
		if (method === "language/status") {
			const p = params as { type?: string; message?: string };
			// jdtls signals readiness via language/status with
			// type="ServiceReady". Do NOT use "Started" — that fires
			// before Gradle synchronization completes, leading to
			// null hovers on cold workspaces.
			if (p?.type === "ServiceReady") {
				this.projectReady = true;
			}
		}
		if (method === "$/progress") {
			const p = params as {
				token?: unknown;
				value?: { kind?: string };
			};
			if (p?.value?.kind === "end") {
				this.projectReady = true;
			}
		}
	}

	/** Check if jdtls has signaled project readiness via progress. */
	isProjectReady(): boolean {
		return this.projectReady;
	}

	private waitForResponse(id: number, timeoutMs: number): Promise<LspResponse | null> {
		return new Promise((resolve) => {
			const timer = setTimeout(() => { this.pendingResolves.delete(id); resolve(null); }, timeoutMs);
			this.pendingResolves.set(id, (resp) => { clearTimeout(timer); resolve(resp); });
		});
	}
}

// ── Type extraction from Java hover ─────────────────────────────────

/** Exported for testing. */
export function extractJavaTypeFromHover(hoverText: string): string | null {
	const stripped = hoverText
		.replace(/```java\n?/g, "")
		.replace(/```\n?/g, "")
		.trim();
	if (!stripped) return null;

	// Pattern 1: variable/field declaration with type.
	// "String name" or "List<String> items" or "HashMap<K,V> map"
	const declMatch = stripped.match(
		/^([A-Z][\w]*(?:<[^>]*>)?)\s+\w+/m,
	);
	if (declMatch) {
		return cleanJavaTypeName(declMatch[1]);
	}

	// Pattern 2: method return type.
	// "public String getName()" → "String"
	const methodMatch = stripped.match(
		/(?:public|private|protected|static|final|\s)+([A-Z][\w]*(?:<[^>]*>)?)\s+\w+\s*\(/,
	);
	if (methodMatch) {
		return cleanJavaTypeName(methodMatch[1]);
	}

	// Pattern 3: plain class/interface/enum reference.
	const classMatch = stripped.match(
		/(?:class|interface|enum)\s+([A-Z][\w]*)/,
	);
	if (classMatch) return classMatch[1];

	// Pattern 4: qualified type name (org.springframework.web.bind.annotation.RestController).
	const qualifiedMatch = stripped.match(
		/([a-z][\w]*(?:\.[a-z][\w]*)*\.([A-Z][\w]*))/,
	);
	if (qualifiedMatch) return qualifiedMatch[2]; // Last segment.

	// Pattern 5: plain PascalCase type name.
	const plainMatch = stripped.match(/^([A-Z][\w]*)/);
	if (plainMatch) return plainMatch[1];

	return null;
}

function cleanJavaTypeName(raw: string): string | null {
	let name = raw.trim();
	// Strip generics.
	const angleIdx = name.indexOf("<");
	if (angleIdx > 0) name = name.slice(0, angleIdx).trim();
	// Strip array brackets.
	name = name.replace(/\[\]/g, "");
	if (!name || name.length < 2) return null;
	// Must start with uppercase (Java type convention).
	if (!/^[A-Z]/.test(name)) return null;
	return name;
}

/** Exported for testing. */
export function isValidJavaTypeName(name: string): boolean {
	if (!name || name.length < 2) return false;
	const REJECT = new Set([
		"var", "void", "null", "this", "super", "new", "return",
		"if", "else", "for", "while", "do", "switch", "case",
		"try", "catch", "finally", "throw", "throws",
		"class", "interface", "enum", "extends", "implements",
		"public", "private", "protected", "static", "final",
		"abstract", "synchronized", "volatile", "transient",
		"import", "package", "assert", "default",
	]);
	if (REJECT.has(name)) return false;
	if (name.includes("\n")) return false;
	// Java primitives.
	const PRIMITIVES = new Set(["int", "long", "double", "float", "boolean", "byte", "short", "char"]);
	if (PRIMITIVES.has(name)) return true;
	if (/^[A-Z]/.test(name)) return true;
	return false;
}

// ── Project root grouping ───────────────────────────────────────────

type Site = {
	edgeUid: string;
	sourceFilePath: string;
	lineStart: number;
	colStart: number;
	targetKey: string;
};

function groupSitesByJavaProjectRoot(
	repoRootPath: string,
	sites: ReadonlyArray<Site>,
): Map<string, Site[]> {
	const cache = new Map<string, string>();
	const groups = new Map<string, Site[]>();
	for (const site of sites) {
		const root = findNearestJavaProjectRoot(site.sourceFilePath, repoRootPath, cache);
		const group = groups.get(root) ?? [];
		group.push(site);
		groups.set(root, group);
	}
	return groups;
}

function findNearestJavaProjectRoot(
	fileRelPath: string,
	repoRoot: string,
	cache: Map<string, string>,
): string {
	let dir = fileRelPath.includes("/")
		? fileRelPath.slice(0, fileRelPath.lastIndexOf("/"))
		: "";
	const uncached: string[] = [];
	while (true) {
		const cached = cache.get(dir);
		if (cached !== undefined) {
			for (const d of uncached) cache.set(d, cached);
			return cached;
		}
		uncached.push(dir);
		const absDir = dir === "" ? repoRoot : join(repoRoot, dir);
		// Check for Gradle or Maven project markers.
		if (
			existsSync(join(absDir, "build.gradle")) ||
			existsSync(join(absDir, "build.gradle.kts")) ||
			existsSync(join(absDir, "pom.xml"))
		) {
			for (const d of uncached) cache.set(d, absDir);
			return absDir;
		}
		if (dir === "") break;
		const slash = dir.lastIndexOf("/");
		dir = slash >= 0 ? dir.slice(0, slash) : "";
	}
	for (const d of uncached) cache.set(d, repoRoot);
	return repoRoot;
}

// ── Public API ──────────────────────────────────────────────────────

export async function resolveJavaReceiverTypes(
	repoRootPath: string,
	sites: ReadonlyArray<Site>,
	onProgress?: (p: EnrichmentProgress) => void,
): Promise<ReceiverTypeResult[]> {
	if (sites.length === 0) return [];
	const emit = onProgress ?? (() => {});

	const groups = groupSitesByJavaProjectRoot(repoRootPath, sites);
	const allResults: ReceiverTypeResult[] = [];
	let totalProcessed = 0;

	for (const [projectRoot, groupSites] of groups) {
		emit({ phase: "starting_jdtls", current: totalProcessed, total: sites.length });

		const client = new JdtlsClient(projectRoot);
		const started = await client.start(projectRoot);
		if (!started) {
			for (const s of groupSites) {
				allResults.push({
					edgeUid: s.edgeUid,
					receiverType: null,
					receiverTypeOrigin: "failed" as const,
					typeDisplayName: null,
					isExternalType: false,
					failureReason: `jdtls failed to start at ${projectRoot}: ${client.startupFailureReason}`,
				});
			}
			totalProcessed += groupSites.length;
			continue;
		}

		// Wait for jdtls to finish project import. Uses two signals:
		// 1. Progress notification with kind="end" (jdtls sends these)
		// 2. Fallback: hover on first site returns non-null
		// jdtls can take 1-5 minutes for Gradle projects on first run.
		emit({ phase: "loading_project", current: totalProcessed, total: sites.length });
		const firstAbsPath = join(repoRootPath, groupSites[0].sourceFilePath);
		let warmedUp = false;
		for (let attempt = 0; attempt < 120; attempt++) {
			// Primary signal: progress ended.
			if (client.isProjectReady()) {
				warmedUp = true;
				break;
			}
			// Secondary signal: hover returns a type (project was already cached).
			if (attempt > 0 && attempt % 6 === 0) {
				const warmup = await client.hover(firstAbsPath, groupSites[0].lineStart, groupSites[0].colStart);
				if (warmup !== null) { warmedUp = true; break; }
			}
			await new Promise((r) => setTimeout(r, 2000));
		}

		if (!warmedUp) {
			const diag = [
				`serverRequests: [${client.serverRequestsSeen.join(", ")}]`,
				`stderrTail: ${client.stderrLines.slice(-5).join(" | ")}`,
				`projectReady: ${client.isProjectReady()}`,
			].join("; ");
			await client.stop();
			for (const s of groupSites) {
				allResults.push({
					edgeUid: s.edgeUid,
					receiverType: null,
					receiverTypeOrigin: "failed" as const,
					typeDisplayName: null,
					isExternalType: false,
					failureReason: `jdtls warm-up timeout. ${diag}`,
				});
			}
			totalProcessed += groupSites.length;
			continue;
		}

		for (const site of groupSites) {
			emit({ phase: "resolving_types", current: totalProcessed, total: sites.length });
			const absPath = join(repoRootPath, site.sourceFilePath);
			const typeName = await client.hover(absPath, site.lineStart, site.colStart);

			if (typeName && isValidJavaTypeName(typeName)) {
				allResults.push({
					edgeUid: site.edgeUid,
					receiverType: typeName,
					receiverTypeOrigin: "compiler",
					typeDisplayName: typeName,
					isExternalType: isJavaExternalType(typeName),
					failureReason: null,
				});
			} else {
				allResults.push({
					edgeUid: site.edgeUid,
					receiverType: null,
					receiverTypeOrigin: "failed",
					typeDisplayName: null,
					isExternalType: false,
					failureReason: typeName ? `type is ${typeName}` : "hover returned no type",
				});
			}
			totalProcessed++;
		}

		await client.stop();
	}

	emit({ phase: "done", current: sites.length, total: sites.length });
	return allResults;
}

function isJavaExternalType(typeName: string): boolean {
	const JAVA_STD = new Set([
		"String", "Integer", "Long", "Double", "Float", "Boolean",
		"Byte", "Short", "Character", "Number", "Object", "Class",
		"Thread", "System", "Math", "StringBuilder", "StringBuffer",
		"Exception", "RuntimeException", "Error", "Throwable",
		"List", "ArrayList", "LinkedList", "Map", "HashMap", "TreeMap",
		"Set", "HashSet", "TreeSet", "Collection", "Collections",
		"Arrays", "Optional", "Stream", "Collectors",
		"Date", "Calendar", "Instant", "Duration", "LocalDate",
		"LocalDateTime", "ZonedDateTime",
		"File", "Path", "Paths", "Files",
		"InputStream", "OutputStream", "Reader", "Writer",
		"BufferedReader", "BufferedWriter", "PrintWriter",
	]);
	return JAVA_STD.has(typeName);
}
