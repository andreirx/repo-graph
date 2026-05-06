/**
 * LM Studio adapter.
 * LM Studio provides an OpenAI-compatible API at localhost:1234/v1.
 */

import { BaseLLMAdapter, LLMRequestOptions } from './BaseLLMAdapter.js';

export interface LMStudioConfig {
  /** Endpoint URL (default: http://localhost:1234/v1) */
  endpoint?: string;
  /** Model name as loaded in LM Studio */
  model: string;
  /** Request timeout in ms (default: 1800000 = 30 min for large context local models) */
  timeoutMs?: number;
  /** Max retries on transient errors (default: 3) */
  maxRetries?: number;
  /** Max concurrent requests (default: 1) */
  concurrency?: number;
}

export class LMStudioAdapter extends BaseLLMAdapter {
  private endpoint: string;
  private model: string;
  private timeoutMs: number;
  private maxRetries: number;

  constructor(config: LMStudioConfig) {
    super(config.concurrency || 1, config.model, 'lmstudio');
    this.endpoint = config.endpoint || 'http://localhost:1234/v1';
    this.model = config.model;
    this.timeoutMs = config.timeoutMs || 1800000; // 30 min default for large context local models
    this.maxRetries = config.maxRetries ?? 3;
  }

  protected async performComplete(
    prompt: string,
    options?: LLMRequestOptions
  ): Promise<string> {
    const body = this.buildRequestBody(prompt, options);
    let lastError: Error | null = null;

    for (let attempt = 1; attempt <= this.maxRetries; attempt++) {
      try {
        if (attempt > 1) {
          const backoff = Math.pow(2, attempt) * 1000;
          this.log(`Retry ${attempt}/${this.maxRetries} in ${backoff}ms...`);
          await new Promise(r => setTimeout(r, backoff));
        }

        this.log(`LLM [${this.adapterName}/${this.model}] (attempt ${attempt})`);
        const result = await this.performStreamedRequest(body);

        // Retry on empty response (model stuck in thinking loop)
        if (!result || result.trim().length === 0) {
          throw new Error('Empty response from model (possible thinking loop)');
        }

        return result;

      } catch (error: unknown) {
        lastError = error instanceof Error ? error : new Error(String(error));
        this.log(`Error (attempt ${attempt}): ${lastError.message}`);
      }
    }

    throw lastError || new Error('Failed after max retries');
  }

  private async performStreamedRequest(body: object): Promise<string> {
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), this.timeoutMs);

    try {
      const response = await fetch(`${this.endpoint}/chat/completions`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
        signal: controller.signal
      });

      clearTimeout(timeoutId);

      if (!response.ok) {
        const err = await response.text();
        throw new Error(`LM Studio API error (${response.status}): ${err}`);
      }

      return await this.readSSEResponse(response);

    } catch (error: unknown) {
      if (error instanceof Error && error.name === 'AbortError') {
        throw new Error(`Request timed out after ${this.timeoutMs}ms`);
      }
      throw error;
    } finally {
      clearTimeout(timeoutId);
    }
  }

  async testConnection(): Promise<boolean> {
    try {
      const res = await fetch(`${this.endpoint}/models`);
      return res.ok;
    } catch {
      return false;
    }
  }

  private buildRequestBody(prompt: string, options?: LLMRequestOptions): object {
    const messages: Array<{ role: string; content: string }> = [];

    if (options?.systemPrompt) {
      messages.push({ role: 'system', content: options.systemPrompt });
    }
    messages.push({ role: 'user', content: prompt });

    const body: Record<string, unknown> = {
      model: this.model,
      messages,
      stream: true,
      temperature: options?.temperature ?? 0.3
    };

    if (options?.maxTokens && options.maxTokens > 0) {
      body.max_tokens = options.maxTokens;
    }

    return body;
  }
}
