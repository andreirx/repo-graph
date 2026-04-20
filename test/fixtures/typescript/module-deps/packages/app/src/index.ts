// App module imports from core using relative path
import { coreService, coreHelper } from "../../core/src/index";

export function appMain(): void {
	const result = coreService();
	const num = coreHelper();
	console.log(result, num);
}
