/**
 * C/C++ runtime builtins corpus.
 *
 * Identifiers: common C standard library functions and C++ standard
 * library types that appear without explicit include in the code model.
 *
 * Module specifiers: C standard headers and common C++ standard headers.
 * The classifier uses these to distinguish system includes from local
 * project includes.
 */

import type { RuntimeBuiltinsSet } from "../../../core/classification/signals.js";

const C_CPP_BUILTINS: readonly string[] = [
	// C standard library functions
	"printf", "fprintf", "sprintf", "snprintf", "scanf", "sscanf",
	"malloc", "calloc", "realloc", "free",
	"memcpy", "memmove", "memset", "memcmp",
	"strlen", "strcpy", "strncpy", "strcat", "strncat", "strcmp", "strncmp",
	"strstr", "strchr", "strrchr", "strtok",
	"atoi", "atol", "atof", "strtol", "strtoul", "strtod",
	"fopen", "fclose", "fread", "fwrite", "fgets", "fputs",
	"fseek", "ftell", "rewind", "fflush",
	"exit", "abort", "atexit",
	"assert", "errno",
	"NULL", "EOF", "stdin", "stdout", "stderr",
	"size_t", "ssize_t", "ptrdiff_t", "intptr_t", "uintptr_t",
	"int8_t", "int16_t", "int32_t", "int64_t",
	"uint8_t", "uint16_t", "uint32_t", "uint64_t",
	"bool", "true", "false",
	// C++ standard types
	"std", "string", "vector", "map", "set", "unordered_map", "unordered_set",
	"unique_ptr", "shared_ptr", "weak_ptr", "make_unique", "make_shared",
	"pair", "tuple", "optional", "variant", "any",
	"cout", "cin", "cerr", "endl",
	"nullptr", "constexpr", "noexcept",
	// Common std:: qualified calls (classifier recognizes as stdlib)
	"std::sort", "std::find", "std::copy", "std::transform", "std::for_each",
	"std::accumulate", "std::count", "std::count_if",
	"std::remove", "std::remove_if", "std::unique", "std::reverse",
	"std::min", "std::max", "std::swap", "std::move", "std::forward",
	"std::make_unique", "std::make_shared", "std::make_pair", "std::make_tuple",
	"std::get", "std::tie",
	"std::begin", "std::end", "std::next", "std::prev", "std::advance",
	"std::to_string", "std::stoi", "std::stol", "std::stod",
	"std::getline", "std::lock_guard", "std::unique_lock",
	"std::thread", "std::async",
];

const C_CPP_SYSTEM_HEADERS: readonly string[] = [
	// C standard headers
	"stdio.h", "stdlib.h", "string.h", "math.h", "ctype.h",
	"errno.h", "signal.h", "stdarg.h", "stddef.h", "stdint.h",
	"stdbool.h", "limits.h", "float.h", "assert.h", "time.h",
	"locale.h", "setjmp.h", "inttypes.h",
	// POSIX headers
	"unistd.h", "fcntl.h", "sys/types.h", "sys/stat.h", "sys/wait.h",
	"sys/socket.h", "sys/ioctl.h", "sys/mman.h", "sys/mount.h",
	"pthread.h", "semaphore.h", "signal.h", "dirent.h", "dlfcn.h",
	"netinet/in.h", "arpa/inet.h", "netdb.h",
	"getopt.h", "libgen.h", "fnmatch.h", "glob.h",
	// C++ standard headers
	"iostream", "fstream", "sstream", "iomanip",
	"string", "vector", "map", "set", "list", "deque", "queue", "stack",
	"unordered_map", "unordered_set",
	"algorithm", "numeric", "functional", "iterator",
	"memory", "utility", "tuple", "optional", "variant", "any",
	"type_traits", "typeinfo", "typeindex",
	"exception", "stdexcept", "system_error",
	"thread", "mutex", "condition_variable", "future", "atomic",
	"chrono", "ratio",
	"array", "bitset", "complex", "random",
	"regex", "filesystem",
	"cstdio", "cstdlib", "cstring", "cmath", "cctype",
	"cassert", "cerrno", "climits", "cfloat", "cstdint",
];

export const C_CPP_RUNTIME_BUILTINS: RuntimeBuiltinsSet = {
	identifiers: Object.freeze(C_CPP_BUILTINS),
	moduleSpecifiers: Object.freeze(C_CPP_SYSTEM_HEADERS),
};
