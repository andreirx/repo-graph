//! Boundary interaction call detector for C.
//!
//! Detects IPC and inter-device boundary function calls and extracts
//! the context needed for classification (socket family, mmap flags,
//! mknod mode).
//!
//! This detector uses Option A: macro identifier recognition only.
//! No numeric fallback, no variable chasing, no speculative evaluation.
//! If the extractor cannot prove the signal from syntax, it leaves
//! the field unset and lets the emitter decline.
//!
//! Contract: docs/TECH-DEBT.md "Boundary Interaction Extraction — Slice 1A"

use repo_graph_classification::types::SourceLocation;

/// Socket address family detected from function arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketFamily {
    /// AF_UNIX / AF_LOCAL
    Unix,
    /// AF_INET
    Inet,
    /// AF_INET6
    Inet6,
    /// AF_CAN
    Can,
}

/// mmap sharing flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MmapFlags {
    /// MAP_SHARED detected
    Shared,
    /// MAP_PRIVATE detected
    Private,
}

/// Raw boundary call site extracted from C source.
///
/// This is the extractor-side DTO. The emitter converts this to
/// `BoundaryCallsite` from boundary-interaction-extractor.
#[derive(Debug, Clone)]
pub struct RawBoundaryCall {
    /// Function name (e.g., "socket", "bind", "shm_open").
    pub function_name: String,

    /// Source location of the call expression.
    pub location: SourceLocation,

    /// Enclosing function name (for stable key construction).
    pub enclosing_function: String,

    /// Socket family if this is a socket-related call.
    /// Only populated when syntactically provable from argument.
    pub socket_family: Option<SocketFamily>,

    /// mmap flags if this is an mmap call.
    /// Only populated when MAP_SHARED or MAP_PRIVATE is syntactically present.
    pub mmap_flags: Option<MmapFlags>,

    /// mknod mode if this is an mknod call.
    /// Only populated when S_IFIFO is syntactically present.
    pub mknod_mode: Option<u32>,

    /// Extracted path/address argument if syntactically provable.
    /// For bind/connect: only populated when sockaddr_un path is provable.
    pub extracted_argument: Option<String>,

    /// Argument index where the value was extracted.
    pub argument_index: Option<usize>,
}

/// Set of known boundary interaction functions for Slice 1A.
const SOCKET_FUNCTIONS: &[&str] = &["socket"];
const SOCKET_OP_FUNCTIONS: &[&str] = &["bind", "connect", "listen", "accept", "send", "recv", "sendto", "recvfrom"];
const PIPE_FUNCTIONS: &[&str] = &["pipe", "pipe2", "mkfifo", "mknod"];
const SHM_FUNCTIONS: &[&str] = &["shm_open", "shm_unlink", "mmap", "munmap"];
const MQUEUE_FUNCTIONS: &[&str] = &["mq_open", "mq_close", "mq_unlink", "mq_send", "mq_receive"];

/// Check if a function name is a boundary interaction candidate.
pub fn is_boundary_function(name: &str) -> bool {
    SOCKET_FUNCTIONS.contains(&name)
        || SOCKET_OP_FUNCTIONS.contains(&name)
        || PIPE_FUNCTIONS.contains(&name)
        || SHM_FUNCTIONS.contains(&name)
        || MQUEUE_FUNCTIONS.contains(&name)
}

/// Extract boundary calls from a parsed C file.
///
/// `root` is the tree-sitter root node for the file.
/// `src` is the source text as bytes.
/// `file_path` is the repo-relative path (for diagnostics).
///
/// Returns a list of detected boundary calls with extracted context.
pub fn extract_boundary_calls(
    root: &tree_sitter::Node,
    src: &[u8],
    _file_path: &str,
) -> Vec<RawBoundaryCall> {
    let mut results = Vec::new();
    let mut enclosing_function = String::new();

    extract_from_node(root, src, &mut enclosing_function, &mut results);

    results
}

fn extract_from_node(
    node: &tree_sitter::Node,
    src: &[u8],
    enclosing_function: &mut String,
    results: &mut Vec<RawBoundaryCall>,
) {
    match node.kind() {
        "function_definition" => {
            // Update enclosing function name
            if let Some(declarator) = node.child_by_field_name("declarator") {
                *enclosing_function = extract_function_name_from_declarator(&declarator, src);
            }

            // Recurse into body
            if let Some(body) = node.child_by_field_name("body") {
                extract_from_node(&body, src, enclosing_function, results);
            }

            // Clear enclosing function when exiting
            enclosing_function.clear();
        }

        "call_expression" => {
            if let Some(call) = try_extract_boundary_call(node, src, enclosing_function) {
                results.push(call);
            }

            // Still recurse for nested calls
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                extract_from_node(&child, src, enclosing_function, results);
            }
        }

        // Recurse into preprocessor blocks
        "preproc_ifdef" | "preproc_if" | "preproc_else" | "preproc_elif" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                extract_from_node(&child, src, enclosing_function, results);
            }
        }

        _ => {
            // Don't recurse into nested function definitions
            if node.kind() == "function_definition" {
                return;
            }

            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                extract_from_node(&child, src, enclosing_function, results);
            }
        }
    }
}

fn try_extract_boundary_call(
    node: &tree_sitter::Node,
    src: &[u8],
    enclosing_function: &str,
) -> Option<RawBoundaryCall> {
    let function_node = node.child_by_field_name("function")?;

    // Only handle direct identifier calls
    if function_node.kind() != "identifier" {
        return None;
    }

    let function_name = function_node.utf8_text(src).ok()?.to_string();

    if !is_boundary_function(&function_name) {
        return None;
    }

    let arguments = node.child_by_field_name("arguments")?;
    let args = collect_arguments(&arguments, src);

    let location = location_from_node(node);

    let mut call = RawBoundaryCall {
        function_name: function_name.clone(),
        location,
        enclosing_function: enclosing_function.to_string(),
        socket_family: None,
        mmap_flags: None,
        mknod_mode: None,
        extracted_argument: None,
        argument_index: None,
    };

    // Extract context based on function type
    match function_name.as_str() {
        "socket" => {
            // socket(domain, type, protocol) — domain is arg0
            if let Some(arg0) = args.first() {
                call.socket_family = parse_socket_family(arg0);
            }
        }

        "bind" | "connect" | "listen" | "accept" | "send" | "recv" | "sendto" | "recvfrom" => {
            // These don't have domain argument — we need socket_family from context
            // For now, try to extract path from sockaddr argument if it's a literal
            // This is rare in practice; most code uses variables
            // Leave socket_family = None for now; emitter will require it
        }

        "mmap" => {
            // mmap(addr, length, prot, flags, fd, offset) — flags is arg3
            if args.len() > 3 {
                call.mmap_flags = parse_mmap_flags(&args[3]);
            }
        }

        "mknod" => {
            // mknod(pathname, mode, dev) — mode is arg1
            if args.len() > 1 {
                call.mknod_mode = parse_mknod_mode(&args[1]);
            }
            // Extract pathname from arg0 if it's a string literal
            if let Some(arg0) = args.first() {
                if let Some(path) = extract_string_literal(arg0) {
                    call.extracted_argument = Some(path);
                    call.argument_index = Some(0);
                }
            }
        }

        "mkfifo" => {
            // mkfifo(pathname, mode) — always creates FIFO, no mode check needed
            // mode is for permissions, not file type
            // Set mknod_mode to S_IFIFO to indicate this is a FIFO
            call.mknod_mode = Some(0o010000); // S_IFIFO

            // Extract pathname from arg0
            if let Some(arg0) = args.first() {
                if let Some(path) = extract_string_literal(arg0) {
                    call.extracted_argument = Some(path);
                    call.argument_index = Some(0);
                }
            }
        }

        "pipe" | "pipe2" => {
            // pipe(pipefd) — always creates anonymous pipe
            // No arguments to extract for channel identity
        }

        "shm_open" => {
            // shm_open(name, oflag, mode) — name is arg0
            if let Some(arg0) = args.first() {
                if let Some(name) = extract_string_literal(arg0) {
                    call.extracted_argument = Some(name);
                    call.argument_index = Some(0);
                }
            }
        }

        "shm_unlink" | "munmap" => {
            // shm_unlink(name) — name is arg0
            if let Some(arg0) = args.first() {
                if let Some(name) = extract_string_literal(arg0) {
                    call.extracted_argument = Some(name);
                    call.argument_index = Some(0);
                }
            }
        }

        "mq_open" => {
            // mq_open(name, oflag, ...) — name is arg0
            if let Some(arg0) = args.first() {
                if let Some(name) = extract_string_literal(arg0) {
                    call.extracted_argument = Some(name);
                    call.argument_index = Some(0);
                }
            }
        }

        "mq_close" | "mq_unlink" | "mq_send" | "mq_receive" => {
            // First arg is mqdes (descriptor), not name
            // Can't extract channel identity without tracking
        }

        _ => {}
    }

    Some(call)
}

/// Collect argument text from an argument_list node.
fn collect_arguments(args_node: &tree_sitter::Node, src: &[u8]) -> Vec<String> {
    let mut args = Vec::new();
    let mut cursor = args_node.walk();

    for child in args_node.children(&mut cursor) {
        // Skip punctuation (parens, commas)
        if child.kind() == "(" || child.kind() == ")" || child.kind() == "," {
            continue;
        }

        if let Ok(text) = child.utf8_text(src) {
            args.push(text.trim().to_string());
        }
    }

    args
}

/// Parse socket family from an argument string.
/// Only recognizes explicit macro identifiers.
fn parse_socket_family(arg: &str) -> Option<SocketFamily> {
    let arg = arg.trim();

    // Direct macro match
    match arg {
        "AF_UNIX" | "AF_LOCAL" | "PF_UNIX" | "PF_LOCAL" => Some(SocketFamily::Unix),
        "AF_INET" | "PF_INET" => Some(SocketFamily::Inet),
        "AF_INET6" | "PF_INET6" => Some(SocketFamily::Inet6),
        "AF_CAN" | "PF_CAN" => Some(SocketFamily::Can),
        _ => None, // Unknown or variable — decline
    }
}

/// Parse mmap flags from an argument string.
/// Recognizes MAP_SHARED or MAP_PRIVATE in bitwise-OR expressions.
fn parse_mmap_flags(arg: &str) -> Option<MmapFlags> {
    let arg = arg.trim();

    // Check for presence of MAP_SHARED or MAP_PRIVATE
    // Handle both direct use and bitwise-OR expressions
    let has_shared = arg.contains("MAP_SHARED");
    let has_private = arg.contains("MAP_PRIVATE");

    // Ambiguous if both present
    if has_shared && has_private {
        return None;
    }

    if has_shared {
        Some(MmapFlags::Shared)
    } else if has_private {
        Some(MmapFlags::Private)
    } else {
        None // Neither found — decline
    }
}

/// Parse mknod mode from an argument string.
/// Returns the mode value if S_IFIFO is syntactically present.
fn parse_mknod_mode(arg: &str) -> Option<u32> {
    let arg = arg.trim();

    // Check for S_IFIFO presence
    if arg.contains("S_IFIFO") {
        // Return the S_IFIFO bit — actual mode value doesn't matter
        // for the guard predicate, only whether S_IFIFO is present
        Some(0o010000) // S_IFIFO
    } else {
        None // S_IFIFO not found — decline
    }
}

/// Extract a string literal value, stripping quotes.
fn extract_string_literal(arg: &str) -> Option<String> {
    let arg = arg.trim();

    if arg.starts_with('"') && arg.ends_with('"') && arg.len() >= 2 {
        Some(arg[1..arg.len() - 1].to_string())
    } else {
        None // Not a string literal
    }
}

fn extract_function_name_from_declarator(declarator: &tree_sitter::Node, src: &[u8]) -> String {
    let mut current = *declarator;

    // Unwrap function_declarator and pointer_declarator wrappers
    while current.kind() == "function_declarator" || current.kind() == "pointer_declarator" {
        if let Some(inner) = current.child_by_field_name("declarator") {
            current = inner;
        } else {
            break;
        }
    }

    if current.kind() == "identifier" {
        current.utf8_text(src).unwrap_or("").to_string()
    } else {
        // Try to find identifier child
        let mut cursor = current.walk();
        for child in current.children(&mut cursor) {
            if child.kind() == "identifier" {
                return child.utf8_text(src).unwrap_or("").to_string();
            }
        }
        String::new()
    }
}

fn location_from_node(node: &tree_sitter::Node) -> SourceLocation {
    let start = node.start_position();
    let end = node.end_position();
    SourceLocation {
        line_start: (start.row + 1) as i64,
        col_start: start.column as i64,
        line_end: (end.row + 1) as i64,
        col_end: end.column as i64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_and_extract(source: &str) -> Vec<RawBoundaryCall> {
        let mut parser = tree_sitter::Parser::new();
        let c_lang: tree_sitter::Language = tree_sitter_c::LANGUAGE.into();
        parser.set_language(&c_lang).unwrap();

        let tree = parser.parse(source, None).unwrap();
        extract_boundary_calls(&tree.root_node(), source.as_bytes(), "test.c")
    }

    #[test]
    fn detects_socket_af_unix() {
        let source = r#"
            void server() {
                int fd = socket(AF_UNIX, SOCK_STREAM, 0);
            }
        "#;

        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "socket");
        assert_eq!(calls[0].socket_family, Some(SocketFamily::Unix));
        assert_eq!(calls[0].enclosing_function, "server");
    }

    #[test]
    fn detects_socket_af_inet() {
        let source = r#"
            void client() {
                int fd = socket(AF_INET, SOCK_STREAM, 0);
            }
        "#;

        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].socket_family, Some(SocketFamily::Inet));
    }

    #[test]
    fn socket_with_variable_has_no_family() {
        let source = r#"
            void dynamic_socket(int family) {
                int fd = socket(family, SOCK_STREAM, 0);
            }
        "#;

        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].socket_family, None);
    }

    #[test]
    fn detects_mmap_shared() {
        let source = r#"
            void map_shared() {
                void *p = mmap(NULL, 4096, PROT_READ, MAP_SHARED, fd, 0);
            }
        "#;

        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "mmap");
        assert_eq!(calls[0].mmap_flags, Some(MmapFlags::Shared));
    }

    #[test]
    fn detects_mmap_shared_with_anonymous() {
        let source = r#"
            void map_shared_anon() {
                void *p = mmap(NULL, 4096, PROT_READ | PROT_WRITE, MAP_SHARED | MAP_ANONYMOUS, -1, 0);
            }
        "#;

        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].mmap_flags, Some(MmapFlags::Shared));
    }

    #[test]
    fn mmap_private_is_detected() {
        let source = r#"
            void map_private() {
                void *p = mmap(NULL, 4096, PROT_READ, MAP_PRIVATE, fd, 0);
            }
        "#;

        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].mmap_flags, Some(MmapFlags::Private));
    }

    #[test]
    fn mmap_with_variable_flags_has_none() {
        let source = r#"
            void map_dynamic(int flags) {
                void *p = mmap(NULL, 4096, PROT_READ, flags, fd, 0);
            }
        "#;

        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].mmap_flags, None);
    }

    #[test]
    fn detects_mknod_with_s_ififo() {
        let source = r#"
            void create_fifo() {
                mknod("/tmp/myfifo", S_IFIFO | 0666, 0);
            }
        "#;

        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "mknod");
        assert_eq!(calls[0].mknod_mode, Some(0o010000));
        assert_eq!(calls[0].extracted_argument, Some("/tmp/myfifo".to_string()));
    }

    #[test]
    fn mknod_without_s_ififo_has_none() {
        let source = r#"
            void create_device() {
                mknod("/dev/mydev", S_IFCHR | 0666, dev);
            }
        "#;

        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].mknod_mode, None);
    }

    #[test]
    fn mkfifo_always_sets_s_ififo() {
        let source = r#"
            void create_named_pipe() {
                mkfifo("/tmp/pipe", 0666);
            }
        "#;

        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "mkfifo");
        assert_eq!(calls[0].mknod_mode, Some(0o010000));
        assert_eq!(calls[0].extracted_argument, Some("/tmp/pipe".to_string()));
    }

    #[test]
    fn shm_open_extracts_name() {
        let source = r#"
            void open_shm() {
                int fd = shm_open("/my_shm", O_CREAT | O_RDWR, 0666);
            }
        "#;

        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "shm_open");
        assert_eq!(calls[0].extracted_argument, Some("/my_shm".to_string()));
    }

    #[test]
    fn pipe_call_detected() {
        let source = r#"
            void create_pipe() {
                int pipefd[2];
                pipe(pipefd);
            }
        "#;

        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "pipe");
    }

    #[test]
    fn mq_open_extracts_name() {
        let source = r#"
            void open_queue() {
                mqd_t mq = mq_open("/my_queue", O_CREAT | O_RDWR, 0666, NULL);
            }
        "#;

        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "mq_open");
        assert_eq!(calls[0].extracted_argument, Some("/my_queue".to_string()));
    }

    #[test]
    fn bind_detected_without_family() {
        let source = r#"
            void start_server() {
                bind(fd, (struct sockaddr*)&addr, sizeof(addr));
            }
        "#;

        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "bind");
        assert_eq!(calls[0].socket_family, None); // Can't prove family without context
    }

    #[test]
    fn non_boundary_call_not_detected() {
        let source = r#"
            void do_work() {
                printf("hello\n");
                malloc(100);
            }
        "#;

        let calls = parse_and_extract(source);
        assert!(calls.is_empty());
    }

    #[test]
    fn multiple_calls_in_function() {
        let source = r#"
            void ipc_server() {
                int fd = socket(AF_UNIX, SOCK_STREAM, 0);
                bind(fd, addr, len);
                listen(fd, 5);
            }
        "#;

        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0].function_name, "socket");
        assert_eq!(calls[1].function_name, "bind");
        assert_eq!(calls[2].function_name, "listen");
    }
}
