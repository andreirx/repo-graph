/**
 * Filesystem mutation detector unit tests.
 *
 * Pure functions — no filesystem, no storage.
 * Cross-language coverage: JS/TS, Python, Rust, Java, C/C++.
 * Verifies mutation kind, pattern, and literal vs dynamic path detection.
 */

import { describe, expect, it } from "vitest";
import { detectFsMutations } from "../../../src/core/seams/fs-mutation-detectors.js";

// ── JS/TS ──────────────────────────────────────────────────────────

describe("detectFsMutations — JS/TS", () => {
	it("detects fs.writeFile with literal path", () => {
		const m = detectFsMutations(
			`fs.writeFile("logs/app.log", data, callback);`,
			"src/logger.ts",
		);
		expect(m).toHaveLength(1);
		expect(m[0].mutationKind).toBe("write_file");
		expect(m[0].mutationPattern).toBe("fs_write_file");
		expect(m[0].targetPath).toBe("logs/app.log");
		expect(m[0].dynamicPath).toBe(false);
	});

	it("detects fs.writeFileSync", () => {
		const m = detectFsMutations(
			`fs.writeFileSync("config.json", data);`,
			"src/util.ts",
		);
		expect(m[0].mutationKind).toBe("write_file");
		expect(m[0].targetPath).toBe("config.json");
	});

	it("detects fs.appendFile", () => {
		const m = detectFsMutations(
			`fs.appendFile("audit.log", entry, cb);`,
			"src/audit.ts",
		);
		expect(m[0].mutationKind).toBe("append_file");
		expect(m[0].mutationPattern).toBe("fs_append_file");
	});

	it("detects fs.unlink", () => {
		const m = detectFsMutations(
			`fs.unlink("tmp/cache.json", cb);`,
			"src/cleanup.ts",
		);
		expect(m[0].mutationKind).toBe("delete_path");
		expect(m[0].targetPath).toBe("tmp/cache.json");
	});

	it("detects fs.rm", () => {
		const m = detectFsMutations(
			`fs.rmSync("dist", { recursive: true });`,
			"src/clean.ts",
		);
		expect(m[0].mutationKind).toBe("delete_path");
		expect(m[0].mutationPattern).toBe("fs_rm");
	});

	it("detects fs.mkdir", () => {
		const m = detectFsMutations(
			`fs.mkdirSync("uploads", { recursive: true });`,
			"src/setup.ts",
		);
		expect(m[0].mutationKind).toBe("create_dir");
	});

	it("detects fs.rename and captures destination", () => {
		const m = detectFsMutations(
			`fs.rename("old.txt", "new.txt", cb);`,
			"src/rename.ts",
		);
		expect(m[0].mutationKind).toBe("rename_path");
		expect(m[0].targetPath).toBe("old.txt");
		expect(m[0].destinationPath).toBe("new.txt");
	});

	it("detects fs.copyFile and captures destination", () => {
		const m = detectFsMutations(
			`fs.copyFile("src.txt", "dst.txt", cb);`,
			"src/copy.ts",
		);
		expect(m[0].mutationKind).toBe("copy_path");
		expect(m[0].targetPath).toBe("src.txt");
		expect(m[0].destinationPath).toBe("dst.txt");
	});

	it("detects fs.chmod", () => {
		const m = detectFsMutations(
			`fs.chmodSync("script.sh", 0o755);`,
			"src/perms.ts",
		);
		expect(m[0].mutationKind).toBe("chmod_path");
	});

	it("detects fs.createWriteStream as write_file", () => {
		const m = detectFsMutations(
			`const stream = fs.createWriteStream("output.bin");`,
			"src/stream.ts",
		);
		expect(m[0].mutationKind).toBe("write_file");
		expect(m[0].mutationPattern).toBe("fs_create_write_stream");
	});

	it("marks dynamic path as dynamicPath=true", () => {
		const m = detectFsMutations(
			`fs.writeFile(targetPath, data, cb);`,
			"src/dynamic.ts",
		);
		expect(m).toHaveLength(1);
		expect(m[0].targetPath).toBeNull();
		expect(m[0].dynamicPath).toBe(true);
	});

	it("rejects template literal interpolation as dynamic", () => {
		const m = detectFsMutations(
			"fs.writeFile(`logs/${name}.log`, data, cb);",
			"src/template.ts",
		);
		expect(m[0].targetPath).toBeNull();
		expect(m[0].dynamicPath).toBe(true);
	});

	it("detects multiple mutations in one file", () => {
		const m = detectFsMutations(
			`fs.writeFile("a.txt", data, cb);
fs.unlinkSync("b.txt");`,
			"src/multi.ts",
		);
		expect(m).toHaveLength(2);
		expect(m[0].mutationKind).toBe("write_file");
		expect(m[1].mutationKind).toBe("delete_path");
	});
});

// ── Python ─────────────────────────────────────────────────────────

describe("detectFsMutations — Python", () => {
	it("detects open with write mode", () => {
		const m = detectFsMutations(
			`with open("output.txt", "w") as f: f.write(data)`,
			"src/writer.py",
		);
		expect(m).toHaveLength(1);
		expect(m[0].mutationKind).toBe("write_file");
		expect(m[0].mutationPattern).toBe("py_open_write");
		expect(m[0].targetPath).toBe("output.txt");
	});

	it("detects open with append mode", () => {
		const m = detectFsMutations(
			`open("audit.log", "a")`,
			"src/audit.py",
		);
		expect(m[0].mutationKind).toBe("append_file");
		expect(m[0].mutationPattern).toBe("py_open_append");
	});

	it("detects os.remove", () => {
		const m = detectFsMutations(
			`os.remove("tmp/cache.json")`,
			"src/cleanup.py",
		);
		expect(m[0].mutationKind).toBe("delete_path");
		expect(m[0].targetPath).toBe("tmp/cache.json");
	});

	it("detects shutil.rmtree", () => {
		const m = detectFsMutations(
			`shutil.rmtree("build")`,
			"src/clean.py",
		);
		expect(m[0].mutationKind).toBe("delete_path");
		expect(m[0].mutationPattern).toBe("py_shutil_rmtree");
	});

	it("detects os.makedirs", () => {
		const m = detectFsMutations(
			`os.makedirs("data/uploads")`,
			"src/setup.py",
		);
		expect(m[0].mutationKind).toBe("create_dir");
		expect(m[0].mutationPattern).toBe("py_os_makedirs");
	});

	it("detects pathlib write_text", () => {
		const m = detectFsMutations(
			`Path("config.yaml").write_text(content)`,
			"src/config.py",
		);
		expect(m[0].mutationKind).toBe("write_file");
		expect(m[0].targetPath).toBe("config.yaml");
	});

	it("detects pathlib mkdir", () => {
		const m = detectFsMutations(
			`Path("output").mkdir(parents=True)`,
			"src/setup.py",
		);
		expect(m[0].mutationKind).toBe("create_dir");
	});

	it("detects tempfile creation", () => {
		const m = detectFsMutations(
			`with tempfile.NamedTemporaryFile() as tmp: pass`,
			"src/temp.py",
		);
		expect(m[0].mutationKind).toBe("create_temp");
		expect(m[0].dynamicPath).toBe(true);
	});
});

// ── Rust ───────────────────────────────────────────────────────────

describe("detectFsMutations — Rust", () => {
	it("detects fs::write with literal", () => {
		const m = detectFsMutations(
			`std::fs::write("output.txt", data)?;`,
			"src/main.rs",
		);
		expect(m[0].mutationKind).toBe("write_file");
		expect(m[0].targetPath).toBe("output.txt");
	});

	it("detects fs::remove_file", () => {
		const m = detectFsMutations(
			`fs::remove_file("tmp.dat")?;`,
			"src/clean.rs",
		);
		expect(m[0].mutationKind).toBe("delete_path");
	});

	it("detects fs::create_dir_all", () => {
		const m = detectFsMutations(
			`std::fs::create_dir_all("data/cache")?;`,
			"src/setup.rs",
		);
		expect(m[0].mutationKind).toBe("create_dir");
	});

	it("detects fs::rename and captures destination", () => {
		const m = detectFsMutations(
			`fs::rename("a.txt", "b.txt")?;`,
			"src/rename.rs",
		);
		expect(m[0].mutationKind).toBe("rename_path");
		expect(m[0].targetPath).toBe("a.txt");
		expect(m[0].destinationPath).toBe("b.txt");
	});
});

// ── Java ───────────────────────────────────────────────────────────

describe("detectFsMutations — Java", () => {
	it("detects Files.write with Paths.get literal", () => {
		const m = detectFsMutations(
			`Files.write(Paths.get("output.txt"), bytes);`,
			"src/Main.java",
		);
		expect(m[0].mutationKind).toBe("write_file");
		expect(m[0].targetPath).toBe("output.txt");
	});

	it("detects Files.delete", () => {
		const m = detectFsMutations(
			`Files.delete(Paths.get("tmp.dat"));`,
			"src/Clean.java",
		);
		expect(m[0].mutationKind).toBe("delete_path");
	});

	it("detects new FileOutputStream with literal", () => {
		const m = detectFsMutations(
			`OutputStream out = new FileOutputStream("data.bin");`,
			"src/Writer.java",
		);
		expect(m[0].mutationKind).toBe("write_file");
		expect(m[0].targetPath).toBe("data.bin");
	});
});

// ── C/C++ ──────────────────────────────────────────────────────────

describe("detectFsMutations — C/C++", () => {
	it("detects fopen with write mode", () => {
		const m = detectFsMutations(
			`FILE *f = fopen("data.bin", "wb");`,
			"src/main.c",
		);
		expect(m[0].mutationKind).toBe("write_file");
		expect(m[0].targetPath).toBe("data.bin");
	});

	it("detects fopen with append mode", () => {
		const m = detectFsMutations(
			`FILE *f = fopen("audit.log", "a");`,
			"src/audit.c",
		);
		expect(m[0].mutationKind).toBe("append_file");
	});

	it("detects unlink", () => {
		const m = detectFsMutations(
			`unlink("tmp.dat");`,
			"src/clean.c",
		);
		expect(m[0].mutationKind).toBe("delete_path");
	});

	it("detects mkdir", () => {
		const m = detectFsMutations(
			`mkdir("output", 0755);`,
			"src/setup.c",
		);
		expect(m[0].mutationKind).toBe("create_dir");
	});
});

// ── Edge cases ─────────────────────────────────────────────────────

describe("detectFsMutations — edge cases", () => {
	it("returns empty for unsupported file type", () => {
		const m = detectFsMutations(`fs.writeFile("a", b)`, "src/data.json");
		expect(m).toHaveLength(0);
	});

	it("returns empty for empty content", () => {
		const m = detectFsMutations("", "src/empty.ts");
		expect(m).toHaveLength(0);
	});

	it("does not detect reads", () => {
		const m = detectFsMutations(
			`const data = fs.readFileSync("input.txt");`,
			"src/reader.ts",
		);
		expect(m).toHaveLength(0);
	});

	it("reports correct line numbers", () => {
		const m = detectFsMutations(
			`// line 1
// line 2
fs.writeFile("a.txt", data, cb);`,
			"src/lines.ts",
		);
		expect(m[0].lineNumber).toBe(3);
	});
});
