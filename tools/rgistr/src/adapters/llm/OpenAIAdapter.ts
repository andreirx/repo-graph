/**
 * OpenAI adapter.
 * Standard OpenAI API with retries and SSE streaming.
 */

import { BaseLLMAdapter, LLMRequestOptions } from './BaseLLMAdapter.js';

export interface OpenAIConfig {
  /** OpenAI API key */
  apiKey: string;
  /** Model name (e.g. "gpt-4o-mini", "gpt-4o") */
  model: string;
  /** Endpoint URL (default: https://api.openai.com/v1) */
  endpoint?: string;
  /** Max retries on transient errors (default: 3) */
  maxRetries?: number;
  /** Request timeout in ms (default: 30000) */
  timeoutMs?: number;
  /** Max concurrent requests (default: 1) */
  concurrency?: number;
}

export class OpenAIAdapter extends BaseLLMAdapter {
  private apiKey: string;
  private model: string;
  private endpoint: string;
  private maxRetries: number;
  private timeoutMs: number;

  constructor(config: OpenAIConfig) {
    super(config.concurrency || 1, config.model, 'openai');
    this.apiKey = config.apiKey;
    this.model = config.model;
    this.endpoint = config.endpoint || 'https://api.openai.com/v1';
    this.maxRetries = config.maxRetries ?? 3;
    this.timeoutMs = config.timeoutMs ?? 30000;
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
        return await this.performStreamedRequest(body);

      } catch (error: unknown) {
        lastError = error instanceof Error ? error : new Error(String(error));
        const isFatal = lastError.message.includes('401') ||
                        lastError.message.includes('invalid_api_key');
        if (isFatal) throw lastError;
        this.log(`Error (attempt ${attempt}): ${lastError.message}`);
      }
    }

    throw lastError || new Error('Failed after max retries');
  }

  async testConnection(): Promise<boolean> {
    try {
      const response = await fetch(`${this.endpoint}/models`, {
        method: 'GET',
        headers: { 'Authorization': `Bearer ${this.apiKey}` }
      });
      return response.ok;
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
      body.max_completion_tokens = options.maxTokens;
    }

    if (options?.expectsJSON) {
      body.response_format = { type: 'json_object' };
    }

    return body;
  }

  private async performStreamedRequest(body: object): Promise<string> {
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), this.timeoutMs);

    try {
      const response = await fetch(`${this.endpoint}/chat/completions`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': `Bearer ${this.apiKey}`
        },
        body: JSON.stringify(body),
        signal: controller.signal
      });

      clearTimeout(timeoutId);

      if (!response.ok) {
        const err = await response.text();
        if (response.status === 429) {
          throw new Error(`Rate Limited (429): ${err}`);
        }
        throw new Error(`OpenAI API error (${response.status}): ${err}`);
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
}
