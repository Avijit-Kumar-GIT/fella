import base from './playwright.generative.config.mjs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
export default { ...base, testMatch: ['**/chat-notebook.desktop.spec.mjs'], outputDir: join(tmpdir(), 'fella-chat-notebook-v1-results') };
