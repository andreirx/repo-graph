// IMPORTS-XPART-ENUMERATION-1 live fixture: package B. A RELATIVE import back into package A — the second
// leg of the cross-partition file-import CYCLE (A -> B -> A) resolved entirely by the in-memory overlay.
import { main } from "../../a/src/main";

export function foo(): string {
  return main();
}
