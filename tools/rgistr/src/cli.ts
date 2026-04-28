#!/usr/bin/env node
/**
 * rgistr CLI - Recursive Gist Runner
 * Hierarchical documentation synthesis via LLM.
 */

import { Command } from 'commander';
import * as path from 'node:path';
import * as fs from 'node:fs/promises';
import { ILLMAdapter } from './adapters/llm/index.js';
import { LMStudioAdapter } from './adapters/llm/LMStudioAdapter.js';
import { OllamaAdapter } from './adapters/llm/OllamaAdapter.js';
import { OpenAIAdapter } from './adapters/llm/OpenAIAdapter.js';
import { generate, GenerationStatus } from './core/generator.js';
import { scanDirectory, countCodeFiles } from './core/scanner.js';

const program = new Command();

program
  .name('rgistr')
  .description('Recursive Gist Runner - Generate hierarchical MAP.md documentation via LLM')
  .version('0.2.0');

program
  .command('generate')
  .description('Generate MAP.md files for a codebase')
  .argument('<path>', 'Path to the codebase root (or subtree)')
  .option('-a, --adapter <type>', 'LLM adapter: lmstudio, ollama, openai', 'lmstudio')
  .option('-m, --model <name>', 'Model name (required for ollama/openai)')
  .option('-e, --endpoint <url>', 'Custom endpoint URL')
  .option('--api-key <key>', 'API key (for openai, or use OPENAI_API_KEY env)')
  .option('-d, --max-depth <n>', 'Maximum recursion depth (-1 = unlimited)', '-1')
  .option('-o, --output <filename>', 'Output filename', 'MAP.md')
  .option('--repo-root <path>', 'Repository root for path provenance (defaults to target path)')
  .option('--force', 'Force regeneration even if MAP.md exists', false)
  .option('--dry-run', 'Show what would be generated without writing', false)
  .action(async (targetPath, opts) => {
    try {
      const rootPath = path.resolve(targetPath);

      // Verify path exists
      try {
        await fs.access(rootPath);
      } catch {
        console.error(`Error: Path does not exist: ${rootPath}`);
        process.exit(1);
      }

      // Create LLM adapter
      const llm = createAdapter(opts);
      if (!llm) {
        process.exit(1);
      }

      // Test connection
      console.log(`Testing connection to ${opts.adapter}...`);
      const connected = await llm.testConnection();
      if (!connected) {
        console.error(`Error: Could not connect to ${opts.adapter}`);
        console.error('Make sure the service is running and accessible.');
        process.exit(1);
      }
      console.log(`Connected to ${llm.adapterName}/${llm.modelName}`);

      // Resolve repo root for path provenance
      const repoRoot = opts.repoRoot ? path.resolve(opts.repoRoot) : rootPath;

      // Validate that target is under repo root
      const relativeToRoot = path.relative(repoRoot, rootPath);
      if (relativeToRoot.startsWith('..') || path.isAbsolute(relativeToRoot)) {
        console.error(`Error: Target path must be under --repo-root`);
        console.error(`  Target: ${rootPath}`);
        console.error(`  Repo root: ${repoRoot}`);
        process.exit(1);
      }

      // Run generation
      console.log(`\nGenerating MAP.md files for: ${rootPath}`);
      if (repoRoot !== rootPath) {
        console.log(`Repo root for path provenance: ${repoRoot}`);
      }
      console.log(`Adapter: ${llm.adapterName}, Model: ${llm.modelName}`);
      console.log(`Output: ${opts.output}, Force: ${opts.force}, Dry-run: ${opts.dryRun}\n`);

      const results = await generate({
        llm,
        repoRoot,
        config: {
          rootPath,
          maxDepth: parseInt(opts.maxDepth, 10),
          outputFilename: opts.output,
          force: opts.force,
          dryRun: opts.dryRun
        },
        onProgress: (status) => {
          if (status.phase === 'scanning') {
            process.stdout.write('Scanning...\r');
          } else if (status.phase === 'generating-files' || status.phase === 'generating-folders') {
            const pct = status.total > 0
              ? Math.round((status.processed / status.total) * 100)
              : 0;
            const phaseLabel = status.phase === 'generating-files' ? 'files' : 'folders';
            process.stdout.write(
              `[${pct}%] ${status.processed}/${status.total} (${phaseLabel}) - ${status.current || ''}\r`
            );
          }
        }
      });

      // Summary
      console.log('\n\n--- Generation Complete ---');
      const succeeded = results.filter(r => r.success).length;
      const failed = results.filter(r => !r.success).length;
      console.log(`Generated: ${succeeded}`);
      console.log(`Failed: ${failed}`);

      if (failed > 0) {
        console.log('\nErrors:');
        for (const r of results.filter(r => !r.success)) {
          console.log(`  ${r.path}: ${r.error}`);
        }
      }

    } catch (error) {
      console.error('Error:', error instanceof Error ? error.message : error);
      process.exit(1);
    }
  });

program
  .command('scan')
  .description('Scan a codebase and show structure (without generating)')
  .argument('<path>', 'Path to the codebase root')
  .option('-d, --max-depth <n>', 'Maximum recursion depth', '-1')
  .action(async (targetPath, opts) => {
    try {
      const rootPath = path.resolve(targetPath);
      const root = await scanDirectory(rootPath, {
        maxDepth: parseInt(opts.maxDepth, 10)
      });

      console.log(`Scanned: ${rootPath}\n`);
      printTree(root, '');

      const codeCount = countCodeFiles(root);
      console.log(`\nTotal code files: ${codeCount}`);

    } catch (error) {
      console.error('Error:', error instanceof Error ? error.message : error);
      process.exit(1);
    }
  });

program
  .command('test-connection')
  .description('Test connection to an LLM service')
  .option('-a, --adapter <type>', 'LLM adapter: lmstudio, ollama, openai', 'lmstudio')
  .option('-m, --model <name>', 'Model name')
  .option('-e, --endpoint <url>', 'Custom endpoint URL')
  .option('--api-key <key>', 'API key (for openai)')
  .action(async (opts) => {
    const llm = createAdapter(opts);
    if (!llm) {
      process.exit(1);
    }

    console.log(`Testing connection to ${opts.adapter}...`);
    const connected = await llm.testConnection();

    if (connected) {
      console.log(`Success: Connected to ${llm.adapterName}/${llm.modelName}`);
    } else {
      console.error(`Failed: Could not connect to ${opts.adapter}`);
      process.exit(1);
    }
  });

function createAdapter(opts: {
  adapter: string;
  model?: string;
  endpoint?: string;
  apiKey?: string;
}): ILLMAdapter | null {
  switch (opts.adapter) {
    case 'lmstudio':
      return new LMStudioAdapter({
        endpoint: opts.endpoint,
        model: opts.model || 'local-model'
      });

    case 'ollama':
      if (!opts.model) {
        console.error('Error: --model is required for ollama adapter');
        return null;
      }
      return new OllamaAdapter({
        endpoint: opts.endpoint,
        model: opts.model
      });

    case 'openai':
      const apiKey = opts.apiKey || process.env.OPENAI_API_KEY;
      if (!apiKey) {
        console.error('Error: --api-key or OPENAI_API_KEY env is required for openai adapter');
        return null;
      }
      if (!opts.model) {
        console.error('Error: --model is required for openai adapter');
        return null;
      }
      return new OpenAIAdapter({
        apiKey,
        endpoint: opts.endpoint,
        model: opts.model
      });

    default:
      console.error(`Error: Unknown adapter: ${opts.adapter}`);
      console.error('Available adapters: lmstudio, ollama, openai');
      return null;
  }
}

function printTree(folder: { relativePath: string; files: { relativePath: string }[]; folders: any[]; hasMap: boolean }, indent: string): void {
  const name = folder.relativePath || '(root)';
  const mapIndicator = folder.hasMap ? ' [MAP]' : '';
  console.log(`${indent}${name}/${mapIndicator}`);

  for (const file of folder.files.slice(0, 5)) {
    console.log(`${indent}  ${path.basename(file.relativePath)}`);
  }
  if (folder.files.length > 5) {
    console.log(`${indent}  ... and ${folder.files.length - 5} more files`);
  }

  for (const sub of folder.folders) {
    printTree(sub, indent + '  ');
  }
}

program.parse();
