/**
 * Interface for LLM adapters in rgistr.
 * Provides a unified interface for different LLM providers.
 */
export interface ILLMAdapter {
  /** Human-readable model identifier (e.g. "gpt-4o-mini", "llama3.2:3b") */
  readonly modelName: string;
  /** Adapter type (e.g. "openai", "ollama", "lmstudio") */
  readonly adapterName: string;

  /**
   * Complete a prompt with the configured LLM.
   * @param prompt - The prompt text
   * @param options - Optional configuration
   * @returns The completion text from the LLM
   */
  complete(
    prompt: string,
    options?: CompletionOptions
  ): Promise<string>;

  /**
   * Test the connection to the LLM service.
   * @returns true if connection is successful, false otherwise
   */
  testConnection(): Promise<boolean>;
}

export interface CompletionOptions {
  /** Maximum tokens in the response */
  maxTokens?: number;
  /** Hint that the response should be JSON */
  expectsJSON?: boolean;
  /** Temperature for sampling (0.0 - 1.0) */
  temperature?: number;
  /** System prompt for instruction-tuned models */
  systemPrompt?: string;
}
