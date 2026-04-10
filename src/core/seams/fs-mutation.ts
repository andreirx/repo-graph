/**
 * Filesystem mutation seam model.
 *
 * A filesystem mutation is a fact: "this file mutates target X
 * with operation Y." Identity is (surface, target_path, mutation_kind).
 *
 * Per-call occurrences are preserved as evidence rows.
 *
 * Slice 1 scope: mutation only — writes, deletes, directory creation,
 * rename/copy/chmod, temp creation. Reads are NOT included.
 *
 * Zero external dependencies. Pure domain model.
 */

// ── Mutation kinds ─────────────────────────────────────────────────

/**
 * Coarse mutation taxonomy. Stays queryable without further translation.
 */
export type MutationKind =
	| "write_file"      // overwrite or create with content
	| "append_file"     // append to existing or create
	| "delete_path"     // unlink, remove, rmrf
	| "create_dir"      // mkdir, makedirs, create_dir_all
	| "rename_path"     // rename, move
	| "copy_path"       // copy, copyFile
	| "chmod_path"      // chmod, set permissions
	| "create_temp";    // tempfile creation, mkstemp

/**
 * Which language pattern matched.
 */
export type MutationPattern =
	// JS/TS (node:fs)
	| "fs_write_file"
	| "fs_append_file"
	| "fs_unlink"
	| "fs_rm"
	| "fs_mkdir"
	| "fs_create_write_stream"
	| "fs_rename"
	| "fs_copy_file"
	| "fs_chmod"
	// Python
	| "py_open_write"
	| "py_open_append"
	| "py_os_remove"
	| "py_os_unlink"
	| "py_shutil_rmtree"
	| "py_os_mkdir"
	| "py_os_makedirs"
	| "py_pathlib_write"
	| "py_pathlib_mkdir"
	| "py_tempfile"
	// Rust
	| "rust_fs_write"
	| "rust_fs_remove_file"
	| "rust_fs_remove_dir_all"
	| "rust_fs_create_dir"
	| "rust_fs_rename"
	| "rust_fs_copy"
	// Java
	| "java_files_write"
	| "java_files_delete"
	| "java_files_create_directory"
	| "java_file_output_stream"
	// C/C++
	| "c_fopen_write"
	| "c_fopen_append"
	| "c_unlink"
	| "c_remove"
	| "c_rmdir"
	| "c_mkdir";

// ── Detected mutation (file-local) ─────────────────────────────────

/**
 * One detected mutation occurrence in a source file.
 *
 * For two-ended operations (rename, copy), `targetPath` is the SOURCE
 * path and `destinationPath` is the destination. The identity layer
 * uses `targetPath` so the source-of-mutation semantics are queryable.
 * The destination is preserved as evidence metadata.
 */
export interface DetectedFsMutation {
	/** Repo-relative source file path. */
	readonly filePath: string;
	readonly lineNumber: number;
	readonly mutationKind: MutationKind;
	readonly mutationPattern: MutationPattern;
	/**
	 * Resolved literal target path, or null if dynamic.
	 * For rename/copy, this is the SOURCE path.
	 * Only string-literal first arguments are captured.
	 */
	readonly targetPath: string | null;
	/**
	 * Destination path for two-ended operations (rename, copy).
	 * Null/undefined for single-ended operations or when destination is dynamic.
	 */
	readonly destinationPath?: string | null;
	/** True if the target was a non-literal expression. */
	readonly dynamicPath: boolean;
	readonly confidence: number;
}

// ── Persisted identity ─────────────────────────────────────────────

/**
 * One mutation fact for a surface.
 * Identity: (snapshot_uid, project_surface_uid, target_path, mutation_kind).
 * Only literal-path mutations produce identity rows.
 */
export interface SurfaceFsMutation {
	readonly surfaceFsMutationUid: string;
	readonly snapshotUid: string;
	readonly repoUid: string;
	readonly projectSurfaceUid: string;
	readonly targetPath: string;
	readonly mutationKind: MutationKind;
	readonly confidence: number;
	readonly metadataJson: string | null;
}

// ── Persisted evidence (per-occurrence) ────────────────────────────

/**
 * One source-file occurrence of a mutation.
 * Evidence rows are created for both literal and dynamic paths.
 * Dynamic occurrences have surface_fs_mutation_uid = null.
 */
export interface SurfaceFsMutationEvidence {
	readonly surfaceFsMutationEvidenceUid: string;
	/** Identity row this evidence supports, or null for dynamic-path occurrences. */
	readonly surfaceFsMutationUid: string | null;
	readonly snapshotUid: string;
	readonly repoUid: string;
	readonly projectSurfaceUid: string;
	readonly sourceFilePath: string;
	readonly lineNumber: number;
	readonly mutationKind: MutationKind;
	readonly mutationPattern: MutationPattern;
	/** True if the path was dynamic. */
	readonly dynamicPath: boolean;
	readonly confidence: number;
	readonly metadataJson: string | null;
}
