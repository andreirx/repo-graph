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
    eprintln!("  rmap index   <repo_path> <db_path> [--include-root <path>]...");
    eprintln!("  rmap refresh <repo_path> <db_path> [--include-root <path>]...");
    eprintln!("  rmap trust   <db_path> <repo_uid>");
    eprintln!("  rmap callers <db_path> <repo_uid> <symbol> [--edge-types <types>]");
    eprintln!("  rmap callees <db_path> <repo_uid> <symbol> [--edge-types <types>]");
    eprintln!("  rmap path    <db_path> <repo_uid> <from> <to>");
    eprintln!("  rmap imports <db_path> <repo_uid> <file_path>");
    eprintln!("  rmap violations <db_path> <repo_uid>");
    eprintln!("  rmap gate       <db_path> <repo_uid>");
    eprintln!("  rmap orient     <db_path> <repo_uid> [--budget small|medium|large] [--focus <string>]");
    eprintln!("  rmap check      <db_path> <repo_uid>");
    eprintln!("  rmap churn      <db_path> <repo_uid> [--since <expr>]");
    eprintln!("  rmap hotspots   <db_path> <repo_uid> [--since <expr>] [--exclude-tests] [--exclude-vendored]");
    eprintln!("  rmap coverage   <db_path> <repo_uid> <report_path>");
    eprintln!("  rmap assess     <db_path> <repo_uid> [--baseline <snapshot_uid>]");
    eprintln!("  rmap explain    <db_path> <repo_uid> <target> [--budget medium|large]");
    eprintln!("  rmap docs list    <db_path> <repo_uid>");
    eprintln!("  rmap docs extract <db_path> <repo_uid>");
    eprintln!("  rmap cycles  <db_path> <repo_uid>");
    eprintln!("  rmap stats   <db_path> <repo_uid>");
    eprintln!("  rmap declare boundary <db_path> <repo_uid> <module_path> --forbids <target> [--reason <text>]");
    eprintln!("  rmap declare requirement <db_path> <repo_uid> <req_id> --version <n> --obligation-id <id> --method <method> --obligation <text> [--target <t>] [--threshold <n>] [--operator <op>]");
    eprintln!("  rmap declare quality-policy <db_path> <repo_uid> <policy_id> --measurement <kind> --policy-kind <kind> --threshold <n> [...]");
    eprintln!("  rmap resource readers <db_path> <repo_uid> <resource_stable_key>");
    eprintln!("  rmap resource writers <db_path> <repo_uid> <resource_stable_key>");
    eprintln!("  rmap modules list <db_path> <repo_uid>");
    eprintln!("  rmap modules files <db_path> <repo_uid> <module>");
    eprintln!("  rmap modules deps <db_path> <repo_uid> [module] [--outbound|--inbound]");
    eprintln!("  rmap modules violations <db_path> <repo_uid>");
    eprintln!("  rmap modules boundary <db_path> <repo_uid> <source> --forbids <target> [--reason <text>]");
    eprintln!("  rmap surfaces list <db_path> <repo_uid> [--kind <kind>] [--runtime <rt>] [--source <src>] [--module <m>]");
    eprintln!("  rmap surfaces show <db_path> <repo_uid> <surface_ref>");
    eprintln!("  rmap policy <db_path> <repo_uid> [--kind STATUS_MAPPING|BEHAVIORAL_MARKER] [--file <path>]");
}
