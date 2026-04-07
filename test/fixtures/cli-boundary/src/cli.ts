import { Command } from "commander";

const program = new Command();
program.name("mytool").description("A test CLI tool");

const repo = program.command("repo").description("Repository management");

repo
  .command("add <path>")
  .description("Register a repository")
  .option("--name <name>", "Alias")
  .action((path: string) => {});

repo
  .command("list")
  .description("List repositories")
  .option("--json", "JSON output")
  .action(() => {});

program
  .command("build")
  .description("Build the project")
  .action(() => {});
