// Another app file that imports from core using relative path
import { coreHelper } from "../../core/src/index";

export function formatNumber(): string {
	return coreHelper().toString();
}
