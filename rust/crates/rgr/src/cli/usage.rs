//! Usage help and error formatting.

/// Format a `GateError` using the stderr wording that the
/// pre-relocation `rmap gate` command produced. The
/// relocation changed the error types (gate now returns
/// `GateError` instead of free-form `String` diagnostics), but
/// the CLI test suite pins specific substrings on stderr. This
/// function adapts the new typed errors back to those strings
/// without re-introducing policy coupling in the gate crate.
///
/// When a new operation is added to the gate port, its mapping
/// goes here - not in the gate crate itself, which must stay
/// CLI-agnostic.
pub fn format_gate_error(err: &repo_graph_gate::GateError) -> String {
    use repo_graph_gate::GateError;
    match err {
        GateError::Storage(e) => match e.operation {
            "find_waivers" => format!("failed to read waivers: {}", e.message),
            "get_boundary_declarations" => {
                format!("failed to read boundary declarations: {}", e.message)
            }
            "find_boundary_imports" => {
                format!("failed to query imports between paths: {}", e.message)
            }
            "get_coverage_measurements" => {
                format!("failed to read coverage measurements: {}", e.message)
            }
            "get_complexity_measurements" => {
                format!("failed to read complexity measurements: {}", e.message)
            }
            "get_hotspot_inferences" => {
                format!("failed to read hotspot inferences: {}", e.message)
            }
            // `get_active_requirements` errors bubble up the
            // StorageError's own Display text (which already
            // contains the "malformed requirement ..." wording
            // the old CLI printed).
            _ => e.message.clone(),
        },
        // Malformed measurement/inference rows: the gate
        // assemble layer built the diagnostic string verbatim
        // to match the pre-relocation format
        // ("malformed X measurement for Y: Z" etc.). Passing
        // `reason` directly preserves that.
        GateError::MalformedEvidence { reason, .. } => reason.clone(),
    }
}

/// Print the main usage help to stderr.
pub fn print_usage() {
    eprintln!("usage:");
    eprintln!();
    eprintln!("Indexing (daemon required):");
    eprintln!("  rmap index [repo_path] [--alias <name>] [--include-root <path>]...");
    eprintln!("  rmap refresh [--include-root <path>]...");
    eprintln!();
    eprintln!("Repo management:");
    eprintln!("  rmap repo list                         List all registered repos");
    eprintln!("  rmap repo info [repo] [--json]         Show repo details (default: cwd)");
    eprintln!("  rmap repo alias <repo> <alias>         Set or change alias");
    eprintln!("  rmap repo remove <repo> [--delete-db]  Remove from registry");
    eprintln!();
    eprintln!("Agent orientation (resolve repo from cwd):");
    eprintln!("  rmap orient [--focus <path>] [--budget small|medium|large] [--full]");
    eprintln!("  rmap check [--full]");
    eprintln!("  rmap explain <target> [--budget medium|large] [--full]");
    eprintln!("    --full   uncap output for grep (no budget truncation; no-op on check)");
    eprintln!();
    eprintln!("Graph queries (resolve repo from cwd):");
    eprintln!("  rmap callers <symbol> [--edge-types <types>]");
    eprintln!("  rmap callees <symbol> [--edge-types <types>]");
    eprintln!("  rmap path <from> <to>");
    eprintln!("  rmap imports <file_path>");
    eprintln!("  rmap cycles");
    eprintln!("  rmap stats");
    eprintln!();
    eprintln!("Quality and risk (resolve repo from cwd):");
    eprintln!("  rmap trust");
    eprintln!("  rmap churn [--since <expr>]");
    eprintln!("  rmap hotspots [--since <expr>] [--exclude-tests] [--exclude-vendored]");
    eprintln!("  rmap risk");
    eprintln!("  rmap coverage <report_path>");
    eprintln!("  rmap assess [--baseline <snapshot>]");
    eprintln!();
    eprintln!("Governance (resolve repo from cwd):");
    eprintln!("  rmap violations");
    eprintln!("  rmap gate");
    eprintln!();
    eprintln!("Documentation inventory (resolve repo from cwd):");
    eprintln!("  rmap docs list");
    eprintln!("  rmap docs extract");
    eprintln!();
    eprintln!("Modules (resolve repo from cwd):");
    eprintln!("  rmap modules list");
    eprintln!("  rmap modules files <module>");
    eprintln!("  rmap modules deps [module] [--outbound|--inbound]");
    eprintln!("  rmap modules violations");
    eprintln!();
    eprintln!("Surfaces and boundaries (resolve repo from cwd):");
    eprintln!("  rmap surfaces list [--kind <kind>] [--runtime <rt>] [--module <m>]");
    eprintln!("  rmap surfaces show <surface_ref>");
    eprintln!("  rmap boundaries list [--kind <kind>] [--scope <scope>] [--direction <dir>]");
    eprintln!("  rmap boundaries show <surface_uid>");
    eprintln!("  rmap boundaries summary");
    eprintln!();
    eprintln!("Resources (resolve repo from cwd):");
    eprintln!("  rmap resource list [--kind <kind>]");
    eprintln!("  rmap resource readers <resource_key>");
    eprintln!("  rmap resource writers <resource_key>");
    eprintln!();
    eprintln!("Policy (resolve repo from cwd):");
    eprintln!("  rmap policy [--kind STATUS_MAPPING|BEHAVIORAL_MARKER] [--file <path>]");
    eprintln!();
    eprintln!("Declarations (resolve repo from cwd):");
    eprintln!("  rmap declare boundary <module_path> --forbids <target> [--reason <text>]");
    eprintln!("  rmap declare requirement <req_id> --version <n> --obligation-id <id> ...");
    eprintln!("  rmap declare quality-policy <policy_id> --measurement <kind> ...");
    eprintln!();
    eprintln!("Agent host integration (HOOK-1/HOOK-1A):");
    eprintln!("  rmap hook session-start [--from-stdin | --from-env | --db <path> --repo <path>]");
    eprintln!("  rmap hook prompt-submit [--from-stdin | --from-env | --db <path> --repo <path>]");
    eprintln!("  rmap hook post-edit [--from-stdin | --from-env | --db <path> --repo <path> --files <paths>]");
    eprintln!("  rmap hook pre-compact [--from-stdin | --from-env | --db <path> --repo <path>]");
    eprintln!("  rmap hook stop [--from-stdin | --from-env | --db <path> --repo <path>]");
    eprintln!("  rmap hook status");
    eprintln!();
    eprintln!("Installation management (MAC-1):");
    eprintln!("  rmap doctor [--json]");
    eprintln!("  rmap uninstall [--dry-run] [--force] [--remove-data]");
    eprintln!();
    eprintln!("Host integrations (CLAUDE-1, CODEX-1):");
    eprintln!(
        "  rmap integrate claude-code install [--global|--project] [--full] [--dry-run] [--force]"
    );
    eprintln!("  rmap integrate claude-code remove [--global|--project]");
    eprintln!("  rmap integrate claude-code status [--global|--project] [--json]");
    eprintln!("  rmap integrate codex install [--global|--project] [--full] [--dry-run] [--force]");
    eprintln!("  rmap integrate codex remove [--global|--project]");
    eprintln!("  rmap integrate codex status [--global|--project] [--json]");
}
