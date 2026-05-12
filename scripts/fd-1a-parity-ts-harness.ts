/**
 * FD-1A Parity Harness — run TS Express prototype on corpus
 *
 * Usage: npx tsx scripts/fd-1a-parity-ts-harness.ts
 */

import * as fs from 'fs';
import * as path from 'path';
import { extractExpressRoutes } from '../src/adapters/extractors/typescript/express-route-extractor.js';

const CORPUS_DIR = 'test/fixtures/typescript/express-routes';

interface RouteResult {
  file: string;
  method: string;
  path: string;
  line: number;
  receiver: string;
}

function extractFromFile(filePath: string): RouteResult[] {
  const source = fs.readFileSync(filePath, 'utf-8');
  const relPath = path.relative(process.cwd(), filePath);

  const facts = extractExpressRoutes(source, relPath, 'express-routes', []);

  return facts.map(f => ({
    file: path.basename(relPath),
    method: f.metadata.httpMethod as string,
    path: f.address,
    line: f.lineStart,
    receiver: f.metadata.receiver as string,
  }));
}

function main() {
  const corpusPath = path.resolve(CORPUS_DIR);
  const files = fs.readdirSync(corpusPath)
    .filter(f => f.endsWith('.ts') && f !== 'package.json')
    .map(f => path.join(corpusPath, f));

  const allRoutes: RouteResult[] = [];

  for (const file of files) {
    const routes = extractFromFile(file);
    allRoutes.push(...routes);
  }

  // Sort for consistent comparison
  allRoutes.sort((a, b) => {
    const keyA = `${a.method} ${a.path}`;
    const keyB = `${b.method} ${b.path}`;
    return keyA.localeCompare(keyB);
  });

  const output = {
    command: 'ts-prototype-extract',
    corpus: CORPUS_DIR,
    count: allRoutes.length,
    routes: allRoutes.map(r => ({
      display_name: `${r.method} ${r.path}`,
      entrypoint_path: r.file,
      line: r.line,
      receiver: r.receiver,
    })),
  };

  console.log(JSON.stringify(output, null, 2));
}

main();
