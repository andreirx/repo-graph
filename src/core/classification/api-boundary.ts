/**
 * Boundary interaction model — generic cross-boundary facts.
 *
 * NOT specific to REST/HTTP. This is the universal model for any
 * interaction that crosses a boundary the normal call graph cannot
 * capture: HTTP, RPC/gRPC, IPC/shared-memory, IOCTLs, message
 * queues, events, sockets, device protocols, register maps.
 *
 * Two sides:
 *   - Provider: something exposes an entry/control/data surface
 *   - Consumer: something reaches across the boundary
 *
 * The matcher pairs provider and consumer facts into link candidates.
 * These are stored in a SEPARATE derived table (boundary_links),
 * NOT in the core edges table. Boundary links are protocol-level
 * inferred facts, not language-level extraction edges. They carry
 * mechanism, confidence, and protocol-specific metadata.
 *
 * Transport-specific details live in adapter code, not here.
 * The core model carries only normalized dimensions.
 */

// ── Mechanism ───────────────────────────────────────────────────────

/**
 * The transport/mechanism through which the boundary interaction occurs.
 * Extensible: new mechanisms are added as adapters are built.
 */
export type BoundaryMechanism =
	| "http"
	| "grpc"
	| "rpc"
	| "ipc_shared_memory"
	| "ioctl"
	| "queue"
	| "event"
	| "socket"
	| "device_protocol"
	| "register_map"
	| "cli_command"
	| "other";

/**
 * Role of the boundary participant.
 */
export type BoundaryRole = "provider" | "consumer";

// ── Facts ───────────────────────────────────────────────────────────

/**
 * A boundary provider fact: something exposes an entry surface.
 *
 * Examples:
 *   - Spring @GetMapping("/api/orders/{id}")
 *   - Express app.get("/api/orders/:id")
 *   - gRPC service method OrderService.GetOrder
 *   - IOCTL handler for CMD_SET_MODE
 *   - Shared-memory region provider
 *   - Message topic subscriber
 */
export interface BoundaryProviderFact {
	/** Transport mechanism. */
	mechanism: BoundaryMechanism;
	/** Semantic operation identifier.
	 *  HTTP: "GET /api/orders/{id}"
	 *  gRPC: "OrderService.GetOrder"
	 *  IOCTL: "CMD_SET_MODE"
	 *  Queue: "topic:user.created" */
	operation: string;
	/** Protocol-specific address/path/command.
	 *  HTTP: "/api/orders/{id}"
	 *  gRPC: "order.OrderService/GetOrder"
	 *  IOCTL: "0x1234"
	 *  Shared-memory: "engine_state" */
	address: string;
	/** Stable key of the handler symbol. */
	handlerStableKey: string;
	/** Source file + line. */
	sourceFile: string;
	lineStart: number;
	/** Framework or subsystem that produced this fact. */
	framework: string;
	/** How the fact was determined. */
	basis: "annotation" | "registration" | "convention" | "contract" | "declaration";
	/** Optional: schema/contract/DTO identity if known. */
	schemaRef: string | null;
	/** Mechanism-specific metadata (transport-dependent fields). */
	metadata: Record<string, unknown>;
}

/**
 * A boundary consumer fact: something reaches across the boundary.
 *
 * Examples:
 *   - fetch("/api/orders/123")
 *   - gRPC client stub call
 *   - ioctl(fd, CMD_SET_MODE, ...)
 *   - write to shared-memory region
 *   - publish to message topic
 */
export interface BoundaryConsumerFact {
	/** Transport mechanism. */
	mechanism: BoundaryMechanism;
	/** Semantic operation identifier (same format as provider). */
	operation: string;
	/** Protocol-specific address/path/command. */
	address: string;
	/** Stable key of the calling symbol. */
	callerStableKey: string;
	/** Source file + line. */
	sourceFile: string;
	lineStart: number;
	/** How the fact was determined. */
	basis: "literal" | "template" | "wrapper" | "contract" | "declaration";
	/** Confidence in the extraction (0-1). */
	confidence: number;
	/** Optional: schema/contract identity if known. */
	schemaRef: string | null;
	/** Mechanism-specific metadata. */
	metadata: Record<string, unknown>;
}

// ── Link ────────────────────────────────────────────────────────────

/**
 * A matched boundary link candidate: a provider and consumer were paired.
 *
 * Stored in the `boundary_links` derived table, NOT in the core
 * `edges` table. Derived links are discardable convenience artifacts
 * that can be recomputed from raw facts.
 *
 * Carries stable persisted fact UIDs, not object references.
 * This allows match strategies to normalize, clone, or reconstruct
 * working objects without breaking the link-to-fact association.
 */
export interface BoundaryLinkCandidate {
	/** Persisted UID of the provider fact that was matched. */
	providerFactUid: string;
	/** Persisted UID of the consumer fact that was matched. */
	consumerFactUid: string;
	/** How the match was determined. */
	matchBasis: "exact_contract" | "address_match" | "operation_match" | "heuristic";
	/** Combined confidence. */
	confidence: number;
}
