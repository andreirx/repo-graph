/**
 * Abstract base class for LLM adapters.
 * Handles shared concerns: concurrency, output cleaning, JSON extraction.
 *
 * Concrete adapters implement:
 * - performComplete() - make the actual API call
 * - testConnection() - check if the service is reachable
 */

import { ILLMAdapter, CompletionOptions } from './ILLMAdapter.js';

export interface LLMRequestOptions {
  maxTokens?: number;
  expectsJSON?: boolean;
  temperature?: number;
}

/**
 * Simple semaphore for concurrency control.
 */
class Semaphore {
  private permits: number;
  private waiting: (() => void)[] = [];

  constructor(permits: number) {
    this.permits = permits;
  }

  async acquire(): Promise<void> {
    if (this.permits > 0) {
      this.permits--;
      return;
    }
    return new Promise(resolve => this.waiting.push(resolve));
  }

  release(): void {
    this.permits++;
    const next = this.waiting.shift();
    if (next) {
      this.permits--;
      next();
    }
  }

  async run<T>(fn: () => Promise<T>): Promise<T> {
    await this.acquire();
    try {
      return await fn();
    } finally {
      this.release();
    }
  }
}

export abstract class BaseLLMAdapter implements ILLMAdapter {
  readonly modelName: string;
  readonly adapterName: string;
  protected semaphore: Semaphore;

  constructor(concurrency: number, modelName: string, adapterName: string) {
    this.semaphore = new Semaphore(concurrency);
    this.modelName = modelName;
    this.adapterName = adapterName;
  }

  // ============ PUBLIC API ============

  async complete(prompt: string, options?: CompletionOptions): Promise<string> {
    return this.semaphore.run(async () => {
      const expectsJSON = options?.expectsJSON ?? false;

      const raw = await this.performComplete(prompt, {
        maxTokens: options?.maxTokens,
        expectsJSON,
        temperature: options?.temperature
      });

      if (!raw || raw.trim().length === 0) {
        throw new Error('Received empty response from LLM');
      }

      return expectsJSON ? this.extractJSON(raw) : this.cleanOutput(raw);
    });
  }

  // ============ ABSTRACT ============

  protected abstract performComplete(
    prompt: string,
    options?: LLMRequestOptions
  ): Promise<string>;

  public abstract testConnection(): Promise<boolean>;

  // ============ SHARED HELPERS ============

  /**
   * Clean LLM output: remove thinking tags and code fences.
   */
  protected cleanOutput(text: string): string {
    let clean = text.trim();
    // Remove XML Thinking tags (DeepSeek style)
    clean = clean.replace(/<(think|thought|reasoning)>[\s\S]*?<\/\1>/gi, '');
    // Remove Markdown Code Fences
    const fenceMatch = clean.match(/```(?:markdown|md)?\s*([\s\S]*?)\s*```/);
    if (fenceMatch) clean = fenceMatch[1];
    else clean = clean.replace(/```/g, '');
    return clean.trim();
  }

  /**
   * Extract JSON object from text.
   */
  protected extractJSON(text: string): string {
    const cleaned = this.cleanOutput(text);
    const start = cleaned.indexOf('{');
    const end = cleaned.lastIndexOf('}');
    if (start !== -1 && end !== -1 && end >= start) {
      return cleaned.slice(start, end + 1);
    }
    throw new Error(`No JSON object found in LLM response: "${text.slice(0, 80)}..."`);
  }

  /**
   * Timestamped log output.
   */
  protected log(msg: string): void {
    const ts = new Date().toISOString().split('T')[1].slice(0, -1);
    console.log(`[${ts}] ${msg}`);
  }

  /**
   * Parse an OpenAI-compatible SSE response stream.
   * Used by OpenAI-compatible adapters (OpenAI, LMStudio, MLX).
   */
  protected async readSSEResponse(response: Response): Promise<string> {
    if (!response.body) throw new Error('No response body');

    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    let fullText = '';
    let buffer = '';

    while (true) {
      const { value, done } = await reader.read();
      if (value) {
        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        buffer = done ? '' : lines.pop() || '';

        for (const line of lines) {
          const trimmed = line.trim();
          if (!trimmed || trimmed === 'data: [DONE]') continue;
          if (trimmed.startsWith('data: ')) {
            try {
              const json = JSON.parse(trimmed.slice(6));
              const content = json.choices?.[0]?.delta?.content || '';
              if (content) {
                fullText += content;
                process.stdout.write('.');
              }
            } catch (e) { /* ignore parse errors */ }
          }
        }
      }
      if (done) break;
    }

    // Flush remaining buffer
    if (buffer.trim()) {
      const trimmed = buffer.trim();
      if (trimmed.startsWith('data: ') && trimmed !== 'data: [DONE]') {
        try {
          const json = JSON.parse(trimmed.slice(6));
          const content = json.choices?.[0]?.delta?.content || '';
          if (content) fullText += content;
        } catch (e) { /* ignore */ }
      }
    }

    process.stdout.write('\n');
    return fullText;
  }
}
