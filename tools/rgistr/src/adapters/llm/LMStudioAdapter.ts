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
  /** Request timeout in ms (default: 600000 = 10 min) */
  timeoutMs?: number;
  /** Max concurrent requests (default: 1) */
  concurrency?: number;
}

export class LMStudioAdapter extends BaseLLMAdapter {
  private endpoint: string;
  private model: string;
  private timeoutMs: number;

  constructor(config: LMStudioConfig) {
    super(config.concurrency || 1, config.model, 'lmstudio');
    this.endpoint = config.endpoint || 'http://localhost:1234/v1';
    this.model = config.model;
    this.timeoutMs = config.timeoutMs || 600000;
  }

  protected async performComplete(
    prompt: string,
    options?: LLMRequestOptions
  ): Promise<string> {
    const body = this.buildRequestBody(prompt, options);
    this.log(`LLM [${this.adapterName}/${this.model}]`);

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
