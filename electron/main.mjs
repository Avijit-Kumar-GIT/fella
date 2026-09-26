import { app, BrowserWindow, dialog, ipcMain, nativeTheme, protocol, shell } from 'electron';
import { existsSync, mkdirSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { homedir } from 'node:os';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { EngineClient, assertBinary } from './engine.mjs';
import { checkAndApply } from './update.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..');
let engine;

protocol.registerSchemesAsPrivileged([
	{
		scheme: 'fella',
		privileges: { standard: true, secure: true, supportFetchAPI: true, corsEnabled: true }
	}
]);

function engineCandidates() {
	const names = process.platform === 'win32'
		? ['fella-engine.exe', 'fella.exe', 'fella-engine', 'fella']
		: ['fella-engine', 'fella'];
	const explicit = process.env.FELLA_ENGINE_PATH?.trim();
	const roots = [
		process.resourcesPath,
		join(process.resourcesPath, 'fella-engine'),
		join(root, 'electron', 'engine'),
		join(root, 'src-tauri', 'target', 'release'),
		join(root, 'src-tauri', 'target', 'debug')
	].filter(Boolean);
	const discovered = roots.flatMap((base) => {
		const path = base.endsWith('.exe') || base.endsWith('/fella') || base.endsWith('\\fella')
			? [base]
			: names.map((name) => join(base, name));
		return path;
	});
	return explicit ? [explicit, ...discovered] : discovered;
}

function findEngine() {
	return engineCandidates().find((candidate) => existsSync(candidate));
}

function contentType(pathname) {
	const extension = pathname.toLowerCase().split('.').pop();
	return {
		css: 'text/css; charset=utf-8',
		gif: 'image/gif',
		html: 'text/html; charset=utf-8',
		ico: 'image/x-icon',
		jpeg: 'image/jpeg',
		jpg: 'image/jpeg',
		js: 'text/javascript; charset=utf-8',
		json: 'application/json; charset=utf-8',
		png: 'image/png',
		svg: 'image/svg+xml',
		txt: 'text/plain; charset=utf-8',
		webp: 'image/webp',
		woff: 'font/woff',
		woff2: 'font/woff2'
	}[extension] ?? 'application/octet-stream';
}

function dataDirectory() {
	const configured = process.env.FELLA_DATA_DIR?.trim();
	if (configured) return resolve(configured);
	// Match Tauri's app.path().app_data_dir() for the dev.fella.app
	// identifier. FELLA_DATA_DIR remains available for isolated tests and
	// benchmarks.
	if (process.platform === 'linux') {
		const xdgDataHome = process.env.XDG_DATA_HOME?.trim();
		const dataHome = xdgDataHome ? resolve(xdgDataHome) : join(homedir(), '.local', 'share');
		return join(dataHome, 'dev.fella.app');
	}
	return join(app.getPath('appData'), 'dev.fella.app');
}

const DARK_WINDOW = '#0e0e10';
const LIGHT_WINDOW = '#fcfcfb';

function setWindowAppearance(win, dark) {
	win.setBackgroundColor(dark ? DARK_WINDOW : LIGHT_WINDOW);
}

async function waitForDevServer(url, timeoutMs = 15000) {
	const deadline = Date.now() + timeoutMs;
	let lastError;
	while (Date.now() < deadline) {
		try {
			const response = await fetch(url, { method: 'HEAD', signal: AbortSignal.timeout(1000) });
			if (response.ok || response.status < 500) return;
			lastError = new Error(`development server returned HTTP ${response.status}`);
		} catch (error) {
			lastError = error;
		}
		await new Promise((resolvePromise) => setTimeout(resolvePromise, 200));
	}
	throw new Error(`development server did not become ready at ${url}: ${lastError ?? 'timed out'}`);
}

async function createWindow() {
	const mac = process.platform === 'darwin';
	const windows = process.platform === 'win32';
	const win = new BrowserWindow({
		width: 1280,
		height: 800,
		minWidth: 960,
		minHeight: 640,
		resizable: true,
		show: false,
		// Match the Tauri overlays: Windows is frameless because Fella draws its
		// own controls; Linux and macOS retain the native frame.
		frame: !windows,
		titleBarStyle: mac ? 'hiddenInset' : undefined,
		backgroundColor: nativeTheme.shouldUseDarkColors ? DARK_WINDOW : LIGHT_WINDOW,
		autoHideMenuBar: true,
		title: 'Fella',
		webPreferences: {
			// Sandboxed preloads cannot use ESM imports. Keep this bridge CommonJS
			// so it executes on Electron 20+ without disabling the renderer sandbox.
			preload: join(here, 'preload.cjs'),
			contextIsolation: true,
			nodeIntegration: false,
			sandbox: true
		}
	});
	setWindowAppearance(win, nativeTheme.shouldUseDarkColors);

	win.once('ready-to-show', () => win.show());
	win.webContents.on('preload-error', (_event, preloadPath, error) => {
		console.error(`Fella preload failed (${preloadPath}):`, error);
	});
	win.webContents.setWindowOpenHandler(({ url }) => {
		void openExternal(url);
		return { action: 'deny' };
	});

	const devUrl = process.env.FELLA_ELECTRON_URL;
	try {
		if (devUrl) await waitForDevServer(devUrl);
		await (devUrl ? win.loadURL(devUrl) : win.loadURL('fella://app/'));
	} catch (error) {
		console.error('Fella window failed to load:', error);
		void dialog.showErrorBox('Fella could not start', String(error));
		app.quit();
	}
	return win;
}

async function openExternal(url) {
	try {
		const parsed = new URL(url);
		if (parsed.protocol !== 'https:' && parsed.protocol !== 'http:') return;
		await shell.openExternal(parsed.toString());
	} catch {
		// Never hand arbitrary schemes from the renderer to the OS shell.
	}
}

app.whenReady().then(() => {
	const buildRoot = app.isPackaged
		? resolve(app.getAppPath(), 'build')
		: resolve(root, 'build');
	protocol.handle('fella', async (request) => {
		const pathname = decodeURIComponent(new URL(request.url).pathname || '/index.html');
		const relativePath = pathname.replace(/^[/\\]+/, '') || 'index.html';
		const target = resolve(buildRoot, relativePath);
		const relativeTarget = relative(buildRoot, target);
		const allowed = relativeTarget === '' || (relativeTarget.split(sep)[0] !== '..' && !relativeTarget.startsWith('..' + sep));
		if (!allowed) return new Response('Forbidden', { status: 403 });
		try {
			const body = await readFile(target);
			return new Response(body, { headers: { 'Content-Type': contentType(target) } });
		} catch (error) {
			console.error(`Fella asset not found: ${target}`, error);
			return new Response('Not Found', { status: 404 });
		}
	});

		const binary = findEngine();
		try {
			assertBinary(binary);
			const dataDir = dataDirectory();
			mkdirSync(dataDir, { recursive: true });
			console.info(`Fella data directory: ${dataDir}`);
			engine = new EngineClient(binary, dataDir);
		} catch (error) {
			void dialog.showErrorBox('Fella engine unavailable', String(error));
			app.quit();
			return;
		}

		ipcMain.handle('fella:invoke', (_event, request) => {
			if (request.command === 'update') return checkAndApply(app);
			return engine.request(request.command, request.args ?? {});
		});
		ipcMain.handle('fella:set-window-appearance', (event, dark) => {
			const owner = BrowserWindow.fromWebContents(event.sender);
			if (owner) setWindowAppearance(owner, Boolean(dark));
		});
		ipcMain.handle('fella:ask', (event, request) =>
			engine.request('ask', request.params ?? {}, (item) => {
				if (!event.sender.isDestroyed()) {
					event.sender.send('fella:ask-event', { requestId: request.requestId, event: item });
				}
			})
		);
		ipcMain.handle('fella:pick-folder', async (event) => {
			const owner = BrowserWindow.fromWebContents(event.sender);
			const result = await dialog.showOpenDialog(owner, {
				properties: ['openDirectory'],
				title: 'Choose a folder'
			});
			return result.canceled ? null : result.filePaths[0] ?? null;
		});
		ipcMain.handle('fella:open-external', (_event, url) => openExternal(url));
		ipcMain.handle('fella:window-action', (event, action) => {
			const owner = BrowserWindow.fromWebContents(event.sender);
			if (!owner) return;
			if (action === 'minimize') owner.minimize();
			else if (action === 'toggleMaximize') owner.isMaximized() ? owner.unmaximize() : owner.maximize();
			else if (action === 'close') owner.close();
		});

		void createWindow();
	});

app.on('window-all-closed', () => {
	if (process.platform !== 'darwin') app.quit();
});

app.on('activate', () => {
	if (BrowserWindow.getAllWindows().length === 0) void createWindow();
});

app.on('before-quit', () => {
		engine?.dispose();
	});
