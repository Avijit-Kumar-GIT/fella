import { appendFileSync, existsSync } from 'node:fs';
import { EngineClient } from '../../electron/engine.mjs';

// Loaded with NODE_OPTIONS before electron/main.mjs, where normal Node ESM
// loading is available. Playwright's ElectronApplication.evaluate() runs in a
// VM context without a dynamic-import callback, so importing the engine there
// would prevent the real app from completing startup.
const originalRequest = EngineClient.prototype.request;
const logPath = process.env.FELLA_ACCEPTANCE_LOG;
const enforcePath = process.env.FELLA_ACCEPTANCE_ENFORCE_FILE;
const forbidden = JSON.parse(process.env.FELLA_ACCEPTANCE_FORBIDDEN || '[]');

EngineClient.prototype.request = function (method, params = {}, onEvent) {
	if (logPath) appendFileSync(logPath, `${JSON.stringify({ method, at: Date.now() })}\n`);
	if (enforcePath && existsSync(enforcePath) && forbidden.includes(method)) {
		return Promise.reject(new Error(`Acceptance budget prohibits execution: ${method}`));
	}
	return originalRequest.call(this, method, params, onEvent);
};
