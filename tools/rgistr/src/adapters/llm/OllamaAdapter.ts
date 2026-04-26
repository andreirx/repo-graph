/**
 * Ollama adapter.
 * Ollama uses NDJSON streaming (not SSE).
 */

import { Agent } from 'undici';
import { BaseLLMAdapter, LLMRequestOptions } from './BaseLLMAdapter.js';

export interface OllamaConfig {
  /** Endpoint URL (default: http://localhost:11434) */
  endpoint?: string;
  /** Model name (e.g. "llama3.2:3b", "qwen2.5:7b") */
  model: string;
  /** Request timeout in ms (default: 600000 = 10 min) */
  timeoutMs?: number;
  /** Context window size (default: 32768) */
  numCtx?: number;
  /** Max concurrent requests (default: 1) */
  concurrency?: number;
}

export class OllamaAdapter extends BaseLLMAdapter {
  private endpoint: string;
  private model: string;
  private timeoutMs: number;
  private numCtx: number;

  constructor(config: OllamaConfig) {
    super(config.concurrency || 1, config.model, 'ollama');
    const ep = config.endpoint || 'http://localhost:11434';
    this.endpoint = ep.endsWith('/') ? ep.slice(0, -1) : ep;
    this.model = config.model;
    this.timeoutMs = config.timeoutMs || 600000;
    this.numCtx = config.numCtx || 32768;
  }

  protected async performComplete(
    prompt: string,
    options?: LLMRequestOptions
  ): Promise<string> {
    const body: Record<string, unknown> = {
      model: this.model,
      prompt,
      stream: true,
      options: {
        temperature: options?.temperature ?? 0.3,
        num_ctx: this.numCtx,
        num_predict: options?.maxTokens || 4096
      }
    };

    if (options?.expectsJSON) {
      body.format = 'json';
    }

    this.log(`LLM [${this.adapterName}/${this.model}]`);
    return this.performOllamaStream(body);
  }

  async testConnection(): Promise<boolean> {
    try {
      const res = await fetch(`${this.endpoint}/api/tags`);
      return res.ok;
    } catch {
      return false;
    }
  }

  /**
   * Ollama uses NDJSON streaming.
   * Each line is a JSON object with { response, done }.
   */
  private async performOllamaStream(body: Record<string, unknown>): Promise<string> {
    const dispatcher = new Agent({
      headersTimeout: this.timeoutMs,
      connectTimeout: this.timeoutMs,
      bodyTimeout: 0 // Infinite body timeout for streaming
    });

    try {
      const response = await fetch(`${this.endpoint}/api/generate`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
        // @ts-expect-error dispatcher is Node.js specific
        dispatcher,
        signal: AbortSignal.timeout(this.timeoutMs)
      });

      if (!response.ok) {
        throw new Error(`Ollama API error: ${response.statusText}`);
      }
      if (!response.body) {
        throw new Error('No response body');
      }

      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let fullText = '';
      let done = false;

      while (!done) {
        const { value, done: doneReading } = await reader.read();
        done = doneReading;
        if (value) {
          const chunk = decoder.decode(value, { stream: true });
          const lines = chunk.split('\n');
          for (const line of lines) {
            if (!line.trim()) continue;
            try {
              const json = JSON.parse(line);
              if (json.response) {
                fullText += json.response;
                process.stdout.write('.');
              }
              if (json.done) done = true;
            } catch (e) { /* ignore parse errors */ }
          }
        }
      }

      process.stdout.write('\n');
      return fullText;

    } catch (error) {
      this.log(`LLM Error: ${error instanceof Error ? error.message : String(error)}`);
      throw error;
    }
  }
}
