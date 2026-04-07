// Second file that also exports a function named "greet" —
// creates ambiguity that only import bindings can resolve.
export function greet(name: string): string {
  return `Hey, ${name}!`;
}
