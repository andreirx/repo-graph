// IMPORTS-XPART-ENUMERATION-1 live fixture: package A. A RELATIVE import that points OUT of package A
// into package B. Within partition A this is locally unresolved (B's file is not part of A's package);
// the cross-partition overlay resolves it to B's FILE node once both partitions are loaded.
import { foo } from "../../b/src/foo";

export function main(): string {
  return foo();
}
