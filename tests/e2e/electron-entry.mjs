// Test-only Electron entry: install the no-execution observer before the real
// app imports EngineClient. This keeps instrumentation out of production code.
import './engine-observer.mjs';

await import('../../electron/main.mjs');
