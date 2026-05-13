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

/**
 * Version injected at bundle time from workspace manifest.
 * Falls back to dev version for unbundled development.
 */
declare const __VERSION__: string | undefined;
const VERSION = typeof __VERSION__ !== 'undefined' ? __VERSION__ : '0.0.0-dev';

const program = new Command();

/**
 * OpenAI model routing configuration.
 * Based on quality comparison: GPT-5.4-mini is better for files, GPT-4.1-mini for folders.
 */
const OPENAI_MODELS = {
  file: 'gpt-5.4-mini',    // Better symbol enumeration, prioritized reading hints
  folder: 'gpt-4.1-mini',  // Better folder synthesis, complete Policy Seams
} as const;

program
  .name('rgistr')
  .description('Recursive Gist Runner - Generate hierarchical MAP.md documentation via LLM')
  .version(VERSION);

program
  .command('generate')
  .description('Generate MAP.md files for a codebase')
  .argument('<path>', 'Path to the codebase root (or subtree)')
  .option('-a, --adapter <type>', 'LLM adapter: lmstudio, ollama, openai (required)')
  .option('-m, --model <name>', 'Model name (required for lmstudio/ollama; openai uses auto-routing if omitted)')
  .option('-e, --endpoint <url>', 'Custom endpoint URL')
  .option('--api-key <key>', 'API key (for openai, or use OPENAI_API_KEY env)')
  .option('-d, --max-depth <n>', 'Maximum recursion depth (-1 = unlimited)', '-1')
  .option('-o, --output <filename>', 'Output filename', 'MAP.md')
  .option('--repo-root <path>', 'Repository root for path provenance (defaults to target path)')
  .option('--force', 'Force regeneration even if MAP.md exists', false)
  .option('--dry-run', 'Show what would be generated without writing', false)
  .option('--chunk-size <kb>', 'Chunk size in KB for large files (default: 200, use 50-100 for local models)', '200')
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

      // Discovery-assisted preflight: if no explicit adapter, run discovery
      if (!opts.adapter) {
        await runDiscoveryPreflight();
        return; // runDiscoveryPreflight exits the process
      }

      // Create LLM adapter(s)
      let llm: ILLMAdapter;
      let folderLLM: ILLMAdapter | undefined;

      if (opts.adapter === 'openai' && !opts.model) {
        // OpenAI auto-routing: use optimal models for each scope
        const apiKey = opts.apiKey || process.env.OPENAI_API_KEY;
        if (!apiKey) {
          console.error('Error: --api-key or OPENAI_API_KEY env is required for openai adapter');
          process.exit(1);
        }

        llm = new OpenAIAdapter({
          apiKey,
          endpoint: opts.endpoint,
          model: OPENAI_MODELS.file
        });
        folderLLM = new OpenAIAdapter({
          apiKey,
          endpoint: opts.endpoint,
          model: OPENAI_MODELS.folder
        });

        console.log(`OpenAI auto-routing: files=${OPENAI_MODELS.file}, folders=${OPENAI_MODELS.folder}`);
      } else {
        // Explicit model choice
        const adapter = createAdapter(opts);
        if (!adapter) {
          process.exit(1);
        }
        llm = adapter;
      }

      // Test connection(s)
      console.log(`Testing connection to ${opts.adapter}...`);
      const connected = await llm.testConnection();
      if (!connected) {
        console.error(`Error: Could not connect to ${opts.adapter} (file model: ${llm.modelName})`);
        console.error('Make sure the service is running and accessible.');
        process.exit(1);
      }
      console.log(`Connected to ${llm.adapterName}/${llm.modelName}`);

      // Test folder LLM if using mixed-model routing
      if (folderLLM) {
        const folderConnected = await folderLLM.testConnection();
        if (!folderConnected) {
          console.error(`Error: Could not connect to ${opts.adapter} (folder model: ${folderLLM.modelName})`);
          console.error('Make sure the service is running and the model is available.');
          process.exit(1);
        }
        console.log(`Connected to ${folderLLM.adapterName}/${folderLLM.modelName}`);
      }

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

      // No scanner size limit for generate: all code files must be processed.
      // Files above WHOLE_FILE_THRESHOLD (200KB) use chunked generation.
      // The scanner default (100KB) is for other use cases; CLI overrides it.
      const results = await generate({
        llm,
        folderLLM,
        repoRoot,
        config: {
          rootPath,
          maxDepth: parseInt(opts.maxDepth, 10),
          maxFileSize: Number.MAX_SAFE_INTEGER,
          outputFilename: opts.output,
          force: opts.force,
          dryRun: opts.dryRun,
          chunkSizeKb: parseInt(opts.chunkSize, 10)
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
  .option('-a, --adapter <type>', 'LLM adapter: lmstudio, ollama, openai (required)')
  .option('-m, --model <name>', 'Model name (required)')
  .option('-e, --endpoint <url>', 'Custom endpoint URL')
  .option('--api-key <key>', 'API key (for openai)')
  .action(async (opts) => {
    if (!opts.adapter) {
      console.error('Error: --adapter is required');
      console.error('Available adapters: lmstudio, ollama, openai');
      process.exit(1);
    }
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

program
  .command('discover')
  .description('Discover available LLM providers and models')
  .option('--json', 'Output as JSON for machine consumption', false)
  .option('--local-only', 'Skip cloud providers', false)
  .option('--cloud-only', 'Skip local providers', false)
  .option('--timeout <ms>', 'Probe timeout in milliseconds', '5000')
  .action(async (opts) => {
    // Validate timeout argument
    const timeoutMs = parseInt(opts.timeout, 10);
    if (Number.isNaN(timeoutMs) || timeoutMs <= 0) {
      console.error(`Error: --timeout must be a positive integer, got: ${opts.timeout}`);
      process.exit(2);
    }

    // Validate contradictory flags
    if (opts.localOnly && opts.cloudOnly) {
      console.error('Error: --local-only and --cloud-only are mutually exclusive');
      process.exit(2);
    }

    // Dynamic import to avoid loading discovery module for other commands
    const { discoverProviders, formatDiscoveryReport, formatDiscoveryReportJSON } =
      await import('./support/discovery/index.js');

    try {
      const report = await discoverProviders({
        localOnly: opts.localOnly,
        cloudOnly: opts.cloudOnly,
        probeTimeoutMs: timeoutMs,
      });

      if (opts.json) {
        console.log(formatDiscoveryReportJSON(report));
      } else {
        console.log(formatDiscoveryReport(report));
      }

      // Exit with error if no providers available
      if (report.availableProviders.length === 0) {
        process.exit(1);
      }

    } catch (error) {
      console.error('Discovery failed:', error instanceof Error ? error.message : error);
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
      if (!opts.model) {
        console.error('Error: --model is required for lmstudio adapter');
        return null;
      }
      return new LMStudioAdapter({
        endpoint: opts.endpoint,
        model: opts.model
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

/**
 * Discovery-assisted preflight.
 *
 * Runs provider discovery and prints results. Never auto-proceeds.
 * User must explicitly specify --adapter after seeing available options.
 *
 * Exit codes:
 * - 1: No providers available (nothing to choose from)
 * - 2: Providers available but explicit choice required
 */
async function runDiscoveryPreflight(): Promise<never> {
  const { discoverProviders, formatDiscoveryReport } =
    await import('./support/discovery/index.js');

  console.log('No --adapter specified. Running provider discovery...\n');

  const report = await discoverProviders({
    probeTimeoutMs: 5000,
  });

  // Print the discovery report
  console.log(formatDiscoveryReport(report));
  console.log('');

  if (report.availableProviders.length === 0) {
    // No providers: error exit with guidance (already in report)
    process.exit(1);
  }

  // Providers available: require explicit selection
  console.log('--- Explicit Selection Required ---');
  console.log('');
  console.log('rgistr does not auto-select. Specify adapter explicitly:');
  console.log('');

  // Build example commands from discovered providers
  for (const provider of report.availableProviders) {
    const candidate = provider.candidate;
    let adapterFlag: string;

    if (candidate.transport === 'openai_cloud') {
      adapterFlag = 'openai';
    } else if (candidate.transport === 'ollama') {
      adapterFlag = 'ollama';
    } else {
      // openai_compatible: use flavor or lmstudio as default
      adapterFlag = candidate.flavor === 'lmstudio' ? 'lmstudio' : 'lmstudio';
    }

    // Show first preferred model, or first model, or placeholder
    const preferredModel = provider.models.find(m => m.isPreferred);
    const firstModel = provider.models[0];
    const modelName = preferredModel?.id ?? firstModel?.id ?? '<model>';

    if (adapterFlag === 'openai') {
      // OpenAI supports auto-routing (no --model needed)
      console.log(`  rgistr generate <path> --adapter openai   # auto-routes: files=gpt-5.4-mini, folders=gpt-4.1-mini`);
      console.log(`  rgistr generate <path> --adapter openai --model ${modelName}   # explicit model`);
    } else {
      console.log(`  rgistr generate <path> --adapter ${adapterFlag} --model ${modelName}`);
    }
  }

  console.log('');
  process.exit(2);
}

program.parse();
