// FD-1B validation: Non-React file (should NOT be detected)
// No React import, so no components or hooks should be detected

export function formatCurrency(amount: number): string {
  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD',
  }).format(amount);
}

export function debounce<T extends (...args: unknown[]) => unknown>(
  fn: T,
  delay: number
): T {
  let timeoutId: ReturnType<typeof setTimeout>;
  return ((...args: Parameters<T>) => {
    clearTimeout(timeoutId);
    timeoutId = setTimeout(() => fn(...args), delay);
  }) as T;
}

// Even if this looks like a hook name, it's not in a React file
export function useFormattedDate(date: Date): string {
  return date.toLocaleDateString();
}
