/**
 * Commander CLI command extractor — unit tests (prototype).
 *
 * Tests extraction of Commander.js command registrations as
 * cli_command boundary provider facts.
 */

import { describe, expect, it } from "vitest";
import { extractCommanderCommands } from "../../../../src/adapters/extractors/typescript/commander-command-extractor.js";

const COMMANDER_IMPORT = 'import { Command } from "commander";\n';

function extract(source: string) {
	return extractCommanderCommands(
		COMMANDER_IMPORT + source,
		"src/cli/commands/test.ts",
		"test-repo",
		[],
	);
}

// ── Basic command detection ─────────────────────────────────────────

describe("extractCommanderCommands — basic detection", () => {
	it("detects program.command with description and action", () => {
		const facts = extract(`
const program = new Command();
program
  .command("build")
  .description("Build the project")
  .action(() => {});
`);
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("build");
		expect(facts[0].operation).toBe("CLI build");
		expect(facts[0].mechanism).toBe("cli_command");
		expect(facts[0].framework).toBe("commander");
		expect(facts[0].basis).toBe("registration");
	});

	it("detects command with positional args", () => {
		const facts = extract(`
const program = new Command();
program
  .command("index <repo>")
  .description("Index a repository")
  .action(() => {});
`);
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("index");
		expect(facts[0].metadata.rawCommand).toBe("index <repo>");
		expect(facts[0].metadata.positionalArgs).toEqual(["<repo>"]);
	});

	it("detects command with optional args", () => {
		const facts = extract(`
const program = new Command();
program
  .command("list [filter]")
  .description("List items")
  .action(() => {});
`);
		expect(facts[0].metadata.positionalArgs).toEqual(["[filter]"]);
	});
});

// ── Command path composition ────────────────────────────────────────

describe("extractCommanderCommands — nested commands", () => {
	it("composes parent + child command path", () => {
		const facts = extract(`
const program = new Command();
const repo = program.command("repo").description("Repository management");
repo
  .command("add <path>")
  .description("Register a local repository")
  .action(() => {});
repo
  .command("index <repo>")
  .description("Index a repository")
  .action(() => {});
`);
		// "repo" group + two leaf commands.
		const addFact = facts.find((f) => f.address === "repo add");
		expect(addFact).toBeDefined();
		expect(addFact!.operation).toBe("CLI repo add");

		const indexFact = facts.find((f) => f.address === "repo index");
		expect(indexFact).toBeDefined();
	});

	it("composes two-level nesting", () => {
		const facts = extract(`
const program = new Command();
const graph = program.command("graph").description("Graph queries");
graph
  .command("callers <repo> <symbol>")
  .description("Who calls this?")
  .action(() => {});
`);
		const callersFact = facts.find((f) => f.address === "graph callers");
		expect(callersFact).toBeDefined();
		expect(callersFact!.operation).toBe("CLI graph callers");
	});

	it("does not confuse same-named subcommands under different parents", () => {
		const facts = extract(`
const program = new Command();
const repo = program.command("repo").description("Repo management");
const graph = program.command("graph").description("Graph queries");
repo
  .command("list")
  .description("List repos")
  .action(() => {});
graph
  .command("list")
  .description("List graph items")
  .action(() => {});
`);
		const repoList = facts.find((f) => f.address === "repo list");
		const graphList = facts.find((f) => f.address === "graph list");
		expect(repoList).toBeDefined();
		expect(graphList).toBeDefined();
		expect(repoList!.metadata.description).toBe("List repos");
		expect(graphList!.metadata.description).toBe("List graph items");
	});

	it("same-named subcommands assigned to variables compose correctly", () => {
		const facts = extract(`
const program = new Command();
const repo = program.command("repo").description("Repo management");
const graph = program.command("graph").description("Graph queries");
const repoList = repo.command("list").description("List repos");
const graphList = graph.command("list").description("List graphs");
repoList
  .command("all")
  .description("List all repos")
  .action(() => {});
graphList
  .command("all")
  .description("List all graphs")
  .action(() => {});
`);
		const repoAll = facts.find((f) => f.address === "repo list all");
		const graphAll = facts.find((f) => f.address === "graph list all");
		expect(repoAll).toBeDefined();
		expect(graphAll).toBeDefined();
		expect(repoAll!.metadata.description).toBe("List all repos");
		expect(graphAll!.metadata.description).toBe("List all graphs");
	});
});

// ── Options extraction ──────────────────────────────────────────────

describe("extractCommanderCommands — options", () => {
	it("extracts --json and --limit options", () => {
		const facts = extract(`
const program = new Command();
program
  .command("list")
  .description("List things")
  .option("--json", "JSON output")
  .option("--limit <n>", "Max results")
  .action(() => {});
`);
		expect(facts.length).toBe(1);
		expect(facts[0].metadata.options).toContain("--json");
		expect(facts[0].metadata.options).toContain("--limit <n>");
	});
});

// ── Fact shape ──────────────────────────────────────────────────────

describe("extractCommanderCommands — fact shape", () => {
	it("emits mechanism=cli_command", () => {
		const facts = extract(`
const program = new Command();
program.command("test").description("Run tests").action(() => {});
`);
		expect(facts[0].mechanism).toBe("cli_command");
	});

	it("emits framework=commander", () => {
		const facts = extract(`
const program = new Command();
program.command("test").description("x").action(() => {});
`);
		expect(facts[0].framework).toBe("commander");
	});

	it("emits basis=registration", () => {
		const facts = extract(`
const program = new Command();
program.command("test").description("x").action(() => {});
`);
		expect(facts[0].basis).toBe("registration");
	});

	it("sourceFile matches provided path", () => {
		const facts = extract(`
const program = new Command();
program.command("test").description("x").action(() => {});
`);
		expect(facts[0].sourceFile).toBe("src/cli/commands/test.ts");
	});

	it("metadata.description is present", () => {
		const facts = extract(`
const program = new Command();
program.command("test").description("Run the tests").action(() => {});
`);
		expect(facts[0].metadata.description).toBe("Run the tests");
	});

	it("metadata.hasAction is true for leaf commands", () => {
		const facts = extract(`
const program = new Command();
program.command("test").description("x").action(() => {});
`);
		expect(facts[0].metadata.hasAction).toBe(true);
	});
});

// ── Import gate ─────────────────────────────────────────────────────

describe("extractCommanderCommands — import gate", () => {
	it("returns empty for file without commander import", () => {
		const facts = extractCommanderCommands(
			'const x = program.command("test").description("x").action(() => {});',
			"src/test.ts",
			"test",
			[],
		);
		expect(facts).toEqual([]);
	});
});

// ── No false positives ──────────────────────────────────────────────

describe("extractCommanderCommands — no false positives", () => {
	it("returns empty for non-CLI code", () => {
		const facts = extract("const x = 42;\nexport function foo() {}");
		expect(facts).toEqual([]);
	});

	it("does not emit commands without description or action", () => {
		// A bare .command() with no .description() and no .action()
		// is probably a variable assignment for a group parent.
		// Should only emit if it has a description.
		const facts = extract(`
const program = new Command();
const hidden = program.command("internal");
`);
		expect(facts).toEqual([]);
	});
});

// ── repo-graph self-extraction ──────────────────────────────────────

describe("extractCommanderCommands — repo-graph patterns", () => {
	it("extracts repo-graph boundary command pattern", () => {
		const source = COMMANDER_IMPORT + `
export function registerBoundaryCommands(
  program,
  getCtx,
) {
  const boundary = program
    .command("boundary")
    .description("Boundary interaction facts and links");

  boundary
    .command("summary <repo>")
    .description("Aggregate boundary fact and link counts")
    .option("--json", "JSON output")
    .action((repoRef, opts) => {});

  boundary
    .command("providers <repo>")
    .description("List boundary provider facts")
    .option("--json", "JSON output")
    .option("--mechanism <type>", "Filter by mechanism")
    .action((repoRef, opts) => {});
}
`;
		const facts = extractCommanderCommands(
			source,
			"src/cli/commands/boundary.ts",
			"test-repo",
			[],
		);

		// "boundary" group + "summary" leaf + "providers" leaf.
		expect(facts.length).toBeGreaterThanOrEqual(2);

		const summary = facts.find((f) => f.address === "boundary summary");
		expect(summary).toBeDefined();
		expect(summary!.metadata.options).toContain("--json");

		const providers = facts.find((f) => f.address === "boundary providers");
		expect(providers).toBeDefined();
		expect(providers!.metadata.options).toContain("--json");
		expect(providers!.metadata.options).toContain("--mechanism <type>");
	});
});
