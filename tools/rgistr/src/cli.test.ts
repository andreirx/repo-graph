/**
 * CLI command tests.
 *
 * Tests the public command-line interface behavior.
 */

import { describe, it, expect } from 'vitest';
import { spawnSync } from 'node:child_process';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const CLI_PATH = path.join(__dirname, '..', 'dist', 'cli.js');

/**
 * Run CLI command and return result.
 *
 * Uses spawnSync to properly handle paths with spaces.
 */
function runCli(args: string[]): { stdout: string; stderr: string; exitCode: number } {
  const result = spawnSync('node', [CLI_PATH, ...args], {
    encoding: 'utf-8',
    timeout: 30000,
  });

  return {
    stdout: result.stdout ?? '',
    stderr: result.stderr ?? '',
    exitCode: result.status ?? 1,
  };
}

describe('rgistr discover', () => {
  describe('argument validation', () => {
    it('rejects invalid --timeout value', () => {
      const result = runCli(['discover', '--timeout', 'nope']);

      expect(result.exitCode).toBe(2);
      expect(result.stderr).toContain('--timeout must be a positive integer');
    });

    it('rejects negative --timeout value', () => {
      const result = runCli(['discover', '--timeout', '-1']);

      expect(result.exitCode).toBe(2);
      expect(result.stderr).toContain('--timeout must be a positive integer');
    });

    it('rejects zero --timeout value', () => {
      const result = runCli(['discover', '--timeout', '0']);

      expect(result.exitCode).toBe(2);
      expect(result.stderr).toContain('--timeout must be a positive integer');
    });

    it('rejects contradictory --local-only and --cloud-only', () => {
      const result = runCli(['discover', '--local-only', '--cloud-only']);

      expect(result.exitCode).toBe(2);
      expect(result.stderr).toContain('mutually exclusive');
    });

    it('accepts valid --timeout value', () => {
      // This will likely fail probes (no servers running) but should not error on argument
      const result = runCli(['discover', '--timeout', '100', '--local-only']);

      // Exit code 1 = no providers, not 2 = argument error
      expect(result.exitCode).toBe(1);
      expect(result.stderr).not.toContain('--timeout must be a positive integer');
    });
  });

  describe('output formats', () => {
    it('outputs human-readable format by default', () => {
      const result = runCli(['discover', '--local-only', '--timeout', '100']);

      // Should contain human-readable markers
      expect(result.stdout).toContain('rgistr Provider Discovery');
      expect(result.stdout).toContain('Version:');
      expect(result.stdout).toContain('Probed endpoints:');
    });

    it('outputs valid JSON with --json flag', () => {
      const result = runCli(['discover', '--local-only', '--timeout', '100', '--json']);

      // Should be valid JSON
      expect(() => JSON.parse(result.stdout)).not.toThrow();

      const report = JSON.parse(result.stdout);
      expect(report).toHaveProperty('version');
      expect(report).toHaveProperty('timestamp');
      expect(report).toHaveProperty('providers');
      expect(Array.isArray(report.providers)).toBe(true);
    });

    it('JSON output has required fields', () => {
      const result = runCli(['discover', '--local-only', '--timeout', '100', '--json']);
      const report = JSON.parse(result.stdout);

      // Required top-level fields
      expect(report).toHaveProperty('version');
      expect(report).toHaveProperty('timestamp');
      expect(report).toHaveProperty('notes');
      expect(report).toHaveProperty('providers');
      expect(report).toHaveProperty('selection');

      // Provider fields
      if (report.providers.length > 0) {
        const provider = report.providers[0];
        expect(provider).toHaveProperty('id');
        expect(provider).toHaveProperty('label');
        expect(provider).toHaveProperty('transport');
        expect(provider).toHaveProperty('endpoint');
        expect(provider).toHaveProperty('available');
        expect(provider).toHaveProperty('models');
      }
    });
  });

  describe('exit codes', () => {
    it('exits with 1 when no providers available', () => {
      // With short timeout and local-only, no providers should be found
      const result = runCli(['discover', '--local-only', '--timeout', '100']);

      expect(result.exitCode).toBe(1);
    });

    it('exits with 2 for argument errors', () => {
      const result = runCli(['discover', '--timeout', 'invalid']);

      expect(result.exitCode).toBe(2);
    });
  });

  describe('filtering', () => {
    it('--local-only excludes cloud providers from probing', () => {
      const result = runCli(['discover', '--local-only', '--timeout', '100', '--json']);
      const report = JSON.parse(result.stdout);

      // Should not have any openai_cloud providers
      const cloudProviders = report.providers.filter(
        (p: { transport: string }) => p.transport === 'openai_cloud'
      );
      expect(cloudProviders.length).toBe(0);
    });

    it('--cloud-only excludes local providers from probing', () => {
      // Note: Without OPENAI_API_KEY, this may result in empty providers
      const result = runCli(['discover', '--cloud-only', '--timeout', '100', '--json']);
      const report = JSON.parse(result.stdout);

      // Should not have any local providers
      const localProviders = report.providers.filter(
        (p: { transport: string }) =>
          p.transport === 'openai_compatible' || p.transport === 'ollama'
      );
      expect(localProviders.length).toBe(0);
    });
  });
});

describe('rgistr --version', () => {
  it('outputs version number', () => {
    const result = runCli(['--version']);

    expect(result.exitCode).toBe(0);
    expect(result.stdout).toMatch(/\d+\.\d+\.\d+/);
  });
});

describe('rgistr --help', () => {
  it('outputs help text', () => {
    const result = runCli(['--help']);

    expect(result.exitCode).toBe(0);
    expect(result.stdout).toContain('discover');
    expect(result.stdout).toContain('generate');
    expect(result.stdout).toContain('scan');
  });
});
