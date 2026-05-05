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

/// Socket type detected from function arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketType {
    /// SOCK_STREAM — TCP for AF_INET/AF_INET6, or stream-mode Unix socket.
    Stream,
    /// SOCK_DGRAM — UDP for AF_INET/AF_INET6, or datagram-mode Unix socket.
    Datagram,
    /// SOCK_RAW — raw socket access.
    Raw,
    /// SOCK_SEQPACKET — sequential packet socket.
    SeqPacket,
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

    /// Socket type if this is a socket() call.
    /// Only populated when syntactically provable from argument.
    pub socket_type: Option<SocketType>,

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
/// Signal-related functions for BI-1D.
const SIGNAL_FUNCTIONS: &[&str] = &[
    "kill", "killpg", "raise", "sigqueue", "pthread_kill",
    "signal", "sigaction", "sigwait", "sigwaitinfo", "sigtimedwait", "signalfd",
];
/// SysV shared memory functions for BI-LX-1.
const SYSV_SHM_FUNCTIONS: &[&str] = &["shmget", "shmat", "shmdt", "shmctl"];
/// SysV message queue functions for BI-LX-2.
const SYSV_MSGQ_FUNCTIONS: &[&str] = &["msgget", "msgsnd", "msgrcv", "msgctl"];

/// Check if a function name is a boundary interaction candidate.
pub fn is_boundary_function(name: &str) -> bool {
    SOCKET_FUNCTIONS.contains(&name)
        || SOCKET_OP_FUNCTIONS.contains(&name)
        || PIPE_FUNCTIONS.contains(&name)
        || SHM_FUNCTIONS.contains(&name)
        || MQUEUE_FUNCTIONS.contains(&name)
        || SIGNAL_FUNCTIONS.contains(&name)
        || SYSV_SHM_FUNCTIONS.contains(&name)
        || SYSV_MSGQ_FUNCTIONS.contains(&name)
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
        socket_type: None,
        mmap_flags: None,
        mknod_mode: None,
        extracted_argument: None,
        argument_index: None,
    };

    // Extract context based on function type
    match function_name.as_str() {
        "socket" => {
            // socket(domain, type, protocol) — domain is arg0, type is arg1
            if let Some(arg0) = args.first() {
                call.socket_family = parse_socket_family(arg0);
            }
            if args.len() > 1 {
                call.socket_type = parse_socket_type(&args[1]);
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

        // ── BI-1D: Signal functions ────────────────────────────────────
        "kill" | "killpg" | "sigqueue" | "pthread_kill" => {
            // Signal is arg1 for these functions
            if args.len() > 1 {
                let signal_name = parse_signal_name(&args[1]);
                if let Some(name) = signal_name {
                    call.extracted_argument = Some(name);
                    call.argument_index = Some(1);
                }
            }
        }

        "raise" => {
            // raise(sig) — signal is arg0
            if let Some(arg0) = args.first() {
                if let Some(name) = parse_signal_name(arg0) {
                    call.extracted_argument = Some(name);
                    call.argument_index = Some(0);
                }
            }
        }

        "signal" | "sigaction" => {
            // signal(sig, handler), sigaction(sig, act, oldact) — signal is arg0
            if let Some(arg0) = args.first() {
                if let Some(name) = parse_signal_name(arg0) {
                    call.extracted_argument = Some(name);
                    call.argument_index = Some(0);
                }
            }
        }

        "sigwait" | "sigwaitinfo" | "sigtimedwait" => {
            // sigwait(set, sig) — sigset is arg0, but we can't easily extract
            // the signal from a sigset. Leave extracted_argument empty.
            // Channel identity will be the callsite location.
        }

        "signalfd" => {
            // signalfd(fd, mask, flags) — sigset is arg1
            // Same issue as sigwait — sigset is not a single signal
        }

        // ── BI-LX-2: SysV message queue functions ─────────────────────────
        "msgget" | "msgsnd" | "msgrcv" | "msgctl" => {
            // msgget(key, msgflg) — key is arg0
            // msgsnd(msqid, msgp, msgsz, msgflg) — msqid is runtime
            // msgrcv(msqid, msgp, msgsz, msgtyp, msgflg) — msqid is runtime
            // msgctl(msqid, cmd, buf) — msqid is runtime
            //
            // Only msgget has the key. For others, channel identity requires
            // callsite correlation (deferred). Just detect the call.
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

/// Parse socket type from an argument string.
/// Only recognizes explicit macro identifiers.
/// Handles both direct use and bitwise-OR with flags (e.g., SOCK_STREAM | SOCK_NONBLOCK).
fn parse_socket_type(arg: &str) -> Option<SocketType> {
    let arg = arg.trim();

    // Check for SOCK_* macros, handling bitwise-OR expressions
    // SOCK_NONBLOCK and SOCK_CLOEXEC are flags, not types
    let has_stream = arg.contains("SOCK_STREAM");
    let has_dgram = arg.contains("SOCK_DGRAM");
    let has_raw = arg.contains("SOCK_RAW");
    let has_seqpacket = arg.contains("SOCK_SEQPACKET");

    // Count type indicators (not flags)
    let type_count = [has_stream, has_dgram, has_raw, has_seqpacket]
        .iter()
        .filter(|&&b| b)
        .count();

    // Ambiguous if multiple types present
    if type_count != 1 {
        return None;
    }

    if has_stream {
        Some(SocketType::Stream)
    } else if has_dgram {
        Some(SocketType::Datagram)
    } else if has_raw {
        Some(SocketType::Raw)
    } else if has_seqpacket {
        Some(SocketType::SeqPacket)
    } else {
        None // Should not reach here given type_count == 1
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

/// Parse signal name from an argument string.
/// Recognizes standard POSIX signal names (SIGxxx).
/// Returns the signal name if recognized, None otherwise.
fn parse_signal_name(arg: &str) -> Option<String> {
    let arg = arg.trim();

    // Known POSIX signal names
    const SIGNAL_NAMES: &[&str] = &[
        "SIGABRT", "SIGALRM", "SIGBUS", "SIGCHLD", "SIGCONT",
        "SIGFPE", "SIGHUP", "SIGILL", "SIGINT", "SIGKILL",
        "SIGPIPE", "SIGQUIT", "SIGSEGV", "SIGSTOP", "SIGTERM",
        "SIGTSTP", "SIGTTIN", "SIGTTOU", "SIGUSR1", "SIGUSR2",
        "SIGPOLL", "SIGPROF", "SIGSYS", "SIGTRAP", "SIGURG",
        "SIGVTALRM", "SIGXCPU", "SIGXFSZ",
        // Linux-specific
        "SIGPWR", "SIGSTKFLT", "SIGWINCH",
        // Aliases
        "SIGCLD", "SIGIO",
    ];

    // Check for direct signal name match
    for &sig in SIGNAL_NAMES {
        if arg == sig {
            return Some(sig.to_string());
        }
    }

    // Check if arg contains a signal name (for expressions like SIGTERM | SA_RESTART)
    // Take the first signal name found
    for &sig in SIGNAL_NAMES {
        if arg.contains(sig) {
            return Some(sig.to_string());
        }
    }

    // If arg is a numeric literal, return it as-is
    // This handles cases like kill(pid, 9)
    if arg.chars().all(|c| c.is_ascii_digit()) {
        return Some(arg.to_string());
    }

    None // Unknown or variable — decline
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

    // ── Socket type extraction tests (BI-1B) ──────────────────────────

    #[test]
    fn socket_extracts_sock_stream() {
        let source = r#"
            void tcp_client() {
                int fd = socket(AF_INET, SOCK_STREAM, 0);
            }
        "#;

        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].socket_family, Some(SocketFamily::Inet));
        assert_eq!(calls[0].socket_type, Some(SocketType::Stream));
    }

    #[test]
    fn socket_extracts_sock_dgram() {
        let source = r#"
            void udp_client() {
                int fd = socket(AF_INET, SOCK_DGRAM, 0);
            }
        "#;

        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].socket_family, Some(SocketFamily::Inet));
        assert_eq!(calls[0].socket_type, Some(SocketType::Datagram));
    }

    #[test]
    fn socket_extracts_sock_raw() {
        let source = r#"
            void raw_socket() {
                int fd = socket(AF_INET, SOCK_RAW, IPPROTO_ICMP);
            }
        "#;

        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].socket_type, Some(SocketType::Raw));
    }

    #[test]
    fn socket_extracts_sock_seqpacket() {
        let source = r#"
            void seqpacket_server() {
                int fd = socket(AF_UNIX, SOCK_SEQPACKET, 0);
            }
        "#;

        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].socket_family, Some(SocketFamily::Unix));
        assert_eq!(calls[0].socket_type, Some(SocketType::SeqPacket));
    }

    #[test]
    fn socket_with_flags_extracts_type() {
        // SOCK_STREAM | SOCK_NONBLOCK is a common pattern
        let source = r#"
            void nonblocking_server() {
                int fd = socket(AF_INET6, SOCK_STREAM | SOCK_NONBLOCK, 0);
            }
        "#;

        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].socket_family, Some(SocketFamily::Inet6));
        assert_eq!(calls[0].socket_type, Some(SocketType::Stream));
    }

    #[test]
    fn socket_with_variable_type_has_none() {
        let source = r#"
            void dynamic_socket(int type) {
                int fd = socket(AF_INET, type, 0);
            }
        "#;

        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].socket_family, Some(SocketFamily::Inet));
        assert_eq!(calls[0].socket_type, None); // Can't determine type from variable
    }

    #[test]
    fn unix_socket_stream_extracts_both() {
        let source = r#"
            void unix_server() {
                int fd = socket(AF_UNIX, SOCK_STREAM, 0);
            }
        "#;

        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].socket_family, Some(SocketFamily::Unix));
        assert_eq!(calls[0].socket_type, Some(SocketType::Stream));
    }

    // ── BI-1D: Signal detection tests ─────────────────────────────────

    #[test]
    fn kill_extracts_signal_name() {
        let source = r#"
            void send_term() {
                kill(pid, SIGTERM);
            }
        "#;

        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "kill");
        assert_eq!(calls[0].extracted_argument, Some("SIGTERM".to_string()));
        assert_eq!(calls[0].argument_index, Some(1));
    }

    #[test]
    fn raise_extracts_signal_name() {
        let source = r#"
            void self_signal() {
                raise(SIGUSR1);
            }
        "#;

        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "raise");
        assert_eq!(calls[0].extracted_argument, Some("SIGUSR1".to_string()));
        assert_eq!(calls[0].argument_index, Some(0));
    }

    #[test]
    fn signal_extracts_handler_registration() {
        let source = r#"
            void setup_handler() {
                signal(SIGINT, my_handler);
            }
        "#;

        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "signal");
        assert_eq!(calls[0].extracted_argument, Some("SIGINT".to_string()));
        assert_eq!(calls[0].argument_index, Some(0));
    }

    #[test]
    fn sigaction_extracts_signal_name() {
        let source = r#"
            void setup_sigaction() {
                struct sigaction act;
                sigaction(SIGTERM, &act, NULL);
            }
        "#;

        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "sigaction");
        assert_eq!(calls[0].extracted_argument, Some("SIGTERM".to_string()));
        assert_eq!(calls[0].argument_index, Some(0));
    }

    #[test]
    fn kill_with_numeric_signal() {
        let source = r#"
            void send_signal_9() {
                kill(pid, 9);
            }
        "#;

        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "kill");
        assert_eq!(calls[0].extracted_argument, Some("9".to_string()));
    }

    #[test]
    fn pthread_kill_extracts_signal() {
        let source = r#"
            void cancel_thread() {
                pthread_kill(thread, SIGUSR2);
            }
        "#;

        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "pthread_kill");
        assert_eq!(calls[0].extracted_argument, Some("SIGUSR2".to_string()));
        assert_eq!(calls[0].argument_index, Some(1));
    }

    #[test]
    fn sigqueue_extracts_signal() {
        let source = r#"
            void send_with_data() {
                union sigval val;
                sigqueue(pid, SIGUSR1, val);
            }
        "#;

        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "sigqueue");
        assert_eq!(calls[0].extracted_argument, Some("SIGUSR1".to_string()));
    }

    #[test]
    fn sigwait_is_detected() {
        let source = r#"
            void wait_for_signal() {
                sigset_t set;
                int sig;
                sigwait(&set, &sig);
            }
        "#;

        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "sigwait");
        // sigwait takes a sigset, not a single signal, so extracted_argument is None
        assert_eq!(calls[0].extracted_argument, None);
    }

    #[test]
    fn kill_with_variable_signal_has_none() {
        let source = r#"
            void send_dynamic(int sig) {
                kill(pid, sig);
            }
        "#;

        let calls = parse_and_extract(source);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, "kill");
        assert_eq!(calls[0].extracted_argument, None); // Variable, can't extract
    }
}
