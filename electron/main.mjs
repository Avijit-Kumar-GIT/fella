import { app, BrowserWindow, dialog, ipcMain, Menu, nativeTheme, protocol, screen, shell } from 'electron';
import { mkdirSync, writeFileSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { homedir } from 'node:os';
import { dirname, extname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { EngineClient, assertBinary, isRegularFile } from './engine.mjs';
import { checkAndApply } from './update.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..');
let engine;
let isQuitting = false;
app.setName('Fella');
const hasSingleInstanceLock = app.requestSingleInstanceLock();
if (!hasSingleInstanceLock) {
	app.quit();
} else {
	app.on('second-instance', () => {
		const window = BrowserWindow.getAllWindows()[0];
		if (!window) return;
		if (window.isMinimized()) window.restore();
		if (!window.isVisible()) window.show();
		window.focus();
	});
}

protocol.registerSchemesAsPrivileged([
	{
		scheme: 'fella',
		privileges: { standard: true, secure: true, supportFetchAPI: true, corsEnabled: true }
	}
]);

function engineCandidates() {
	const suffix = process.platform === 'win32' ? '.exe' : '';
	const names = [`fella-engine-${process.arch}${suffix}`, `fella-engine${suffix}`, `fella${suffix}`];
	const explicit = process.env.FELLA_ENGINE_PATH?.trim();
	const roots = [
		process.resourcesPath,
		join(process.resourcesPath, 'fella-engine'),
		join(root, 'electron', 'engine'),
		join(root, 'backend', 'target', 'release'),
		join(root, 'backend', 'target', 'debug')
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
	return engineCandidates().find(isRegularFile);
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
	// Keep the stable dev.fella.app data directory across the shell migration.
	// FELLA_DATA_DIR remains available for isolated tests and
	// benchmarks.
	if (process.platform === 'linux') {
		const xdgDataHome = process.env.XDG_DATA_HOME?.trim();
		const dataHome = xdgDataHome ? resolve(xdgDataHome) : join(homedir(), '.local', 'share');
		return join(dataHome, 'dev.fella.app');
	}
	return join(app.getPath('appData'), 'dev.fella.app');
}

function requestEngine(method, params, onEvent) {
	if (isQuitting) return Promise.resolve(null);
	if (!engine) return Promise.reject(new Error('Fella engine is unavailable.'));
	return engine.request(method, params, (item) => {
		if (!isQuitting) onEvent?.(item);
	}).catch((error) => {
		if (isQuitting) return null;
		throw error;
	});
}

const DARK_WINDOW = '#0e0e10';
const LIGHT_WINDOW = '#fcfcfb';
const WINDOW_STATE_PATH = () => join(app.getPath('userData'), 'window-state.json');

async function readWindowState() {
	try {
		const value = JSON.parse(await readFile(WINDOW_STATE_PATH(), 'utf8'));
		const bounds = value?.bounds;
		if (!bounds || !['x', 'y', 'width', 'height'].every((key) => Number.isFinite(bounds[key]))) return null;
		if (bounds.width < 560 || bounds.height < 640) return null;
		const visible = screen.getAllDisplays().some(({ workArea }) => {
			const width = Math.max(0, Math.min(bounds.x + bounds.width, workArea.x + workArea.width) - Math.max(bounds.x, workArea.x));
			const height = Math.max(0, Math.min(bounds.y + bounds.height, workArea.y + workArea.height) - Math.max(bounds.y, workArea.y));
			return width >= 120 && height >= 120;
		});
		return visible ? { bounds, maximized: value.maximized === true } : null;
	} catch {
		return null;
	}
}

function installApplicationMenu() {
	const sendCommand = (command) => {
		const owner = BrowserWindow.getFocusedWindow() ?? BrowserWindow.getAllWindows()[0];
		if (owner && !owner.webContents.isDestroyed()) owner.webContents.send('fella:app-command', command);
	};
	const item = (label, command, accelerator) => ({
		id: `fella-${command}`,
		label,
		...(accelerator ? { accelerator } : {}),
		click: () => sendCommand(command)
	});
	const mac = process.platform === 'darwin';
	const menus = [];
	if (mac) {
		menus.push({
			label: app.name,
			submenu: [
				{ role: 'about' },
				{ type: 'separator' },
				item('Settings…', 'settings', 'CmdOrCtrl+,'),
				{ type: 'separator' },
				{ role: 'services' },
				{ type: 'separator' },
				{ role: 'hide' },
				{ role: 'hideOthers' },
				{ role: 'unhide' },
				{ type: 'separator' },
				{ role: 'quit' }
			]
		});
	}
	menus.push({
		id: 'fella-menu-file',
		label: 'File',
		submenu: [
			item('New conversation', 'new-conversation', 'CmdOrCtrl+N'),
			item('Open folder…', 'open-folder', 'CmdOrCtrl+O'),
			{ type: 'separator' },
			...(mac ? [] : [item('Settings…', 'settings', 'CmdOrCtrl+,')]),
			{ role: 'close' }
		]
	});
	menus.push({
		id: 'fella-menu-edit',
		label: 'Edit',
		submenu: [
			{ role: 'undo' }, { role: 'redo' }, { type: 'separator' },
			{ role: 'cut' }, { role: 'copy' }, { role: 'paste' }, { role: 'selectAll' }
		]
	});
	menus.push({
		id: 'fella-menu-view',
		label: 'View',
		submenu: [
			item('Search', 'search', 'CmdOrCtrl+K'),
			item('Toggle sidebar', 'toggle-sidebar', 'CmdOrCtrl+B'),
			{ type: 'separator' },
			item('Chat', 'show-chat'),
			item('Notebook', 'open-notebook')
		]
	});
	if (mac) menus.push({ label: 'Window', submenu: [{ role: 'minimize' }, { role: 'zoom' }, { role: 'close' }] });
	menus.push({
		id: 'fella-menu-help',
		role: 'help',
		submenu: [{ label: 'Fella on GitHub', click: () => void openExternal('https://github.com/Avijit-Kumar-GIT/fella') }]
	});
	Menu.setApplicationMenu(Menu.buildFromTemplate(menus));
}

function setWindowAppearance(win, dark) {
	win.setBackgroundColor(dark ? DARK_WINDOW : LIGHT_WINDOW);
	if (process.platform !== 'darwin') {
		win.setTitleBarOverlay({
			color: dark ? DARK_WINDOW : LIGHT_WINDOW,
			symbolColor: dark ? '#a1a1aa' : '#71717a',
			height: 32
		});
	}
}

async function waitForDevServer(url, timeoutMs = 15000) {
	const deadline = Date.now() + timeoutMs;
	let lastError;
	while (Date.now() < deadline) {
		try {
			const response = await fetch(url, { method: 'HEAD', signal: AbortSignal.timeout(1000) });
			const expectedRoot = process.env.FELLA_DEV_ROOT_KEY;
			if ((response.ok || response.status < 500)
				&& (!expectedRoot || response.headers.get('x-fella-dev-root') === expectedRoot)) return;
			lastError = expectedRoot
				? new Error('the response is not from this Fella checkout')
				: new Error(`development server returned HTTP ${response.status}`);
		} catch (error) {
			lastError = error;
		}
		await new Promise((resolvePromise) => setTimeout(resolvePromise, 200));
	}
	throw new Error(`development server did not become ready at ${url}: ${lastError ?? 'timed out'}`);
}

async function createWindow() {
	const savedState = await readWindowState();
	const win = new BrowserWindow({
		...(savedState?.bounds ?? { width: 1280, height: 800 }),
		minWidth: 560,
		minHeight: 640,
		resizable: true,
		show: false,
		// Keep the native resize frame and native system controls while allowing
		// Fella's compact menu/workspace row to share the title bar area.
		frame: true,
		titleBarStyle: 'hidden',
		titleBarOverlay: process.platform === 'darwin'
			? true
			: { color: nativeTheme.shouldUseDarkColors ? DARK_WINDOW : LIGHT_WINDOW, symbolColor: nativeTheme.shouldUseDarkColors ? '#a1a1aa' : '#71717a', height: 32 },
		backgroundColor: nativeTheme.shouldUseDarkColors ? DARK_WINDOW : LIGHT_WINDOW,
		autoHideMenuBar: process.platform !== 'darwin',
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
	if (process.platform !== 'darwin') win.setMenuBarVisibility(false);
	if (savedState?.maximized) win.maximize();
	let windowStateTimer;
	const saveWindowState = () => {
		clearTimeout(windowStateTimer);
		windowStateTimer = setTimeout(() => {
			try {
				const path = WINDOW_STATE_PATH();
				mkdirSync(dirname(path), { recursive: true });
				writeFileSync(path, JSON.stringify({ bounds: win.getNormalBounds(), maximized: win.isMaximized() }), 'utf8');
			} catch (error) {
				console.warn('Could not save Fella window size:', error);
			}
		}, 250);
	};
	win.on('resize', saveWindowState);
	win.on('move', saveWindowState);
	win.on('close', () => {
		clearTimeout(windowStateTimer);
		try {
			const path = WINDOW_STATE_PATH();
			mkdirSync(dirname(path), { recursive: true });
			writeFileSync(path, JSON.stringify({ bounds: win.getNormalBounds(), maximized: win.isMaximized() }), 'utf8');
		} catch (error) {
			console.warn('Could not save Fella window size:', error);
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

function isTrustedAppFrame(event) {
	if (!event.senderFrame || event.senderFrame !== event.sender.mainFrame) return false;
	try {
		const frameUrl = new URL(event.senderFrame.url);
		if (frameUrl.protocol === 'fella:' && frameUrl.hostname === 'app') return true;
		const devUrl = process.env.FELLA_ELECTRON_URL;
		return Boolean(devUrl) && new URL(devUrl).origin === frameUrl.origin;
	} catch {
		return false;
	}
}

app.whenReady().then(() => {
	if (!hasSingleInstanceLock) return;
	installApplicationMenu();
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
			if (error?.code === 'ENOENT' && extname(relativeTarget) === '') {
				try {
					const fallback = join(buildRoot, 'fella-fallback.html');
					return new Response(await readFile(fallback), {
						headers: { 'Content-Type': 'text/html; charset=utf-8' }
					});
				} catch (fallbackError) {
					console.error(`Fella fallback asset not found: ${buildRoot}`, fallbackError);
				}
			}
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
			return requestEngine(request.command, request.args ?? {});
		});
		ipcMain.handle('fella:open-workspace', (event, request) =>
			requestEngine('open_workspace', { path: request.path }, (item) => {
				if (!event.sender.isDestroyed()) {
					event.sender.send('fella:workspace-progress', {
						requestId: request.requestId,
						event: item
					});
				}
			})
		);
		ipcMain.handle('fella:set-window-appearance', (event, dark) => {
			const owner = BrowserWindow.fromWebContents(event.sender);
			if (owner) setWindowAppearance(owner, Boolean(dark));
		});
		ipcMain.handle('fella:popup-menu', (event, request) => {
			if (!isTrustedAppFrame(event)) return false;
			const owner = BrowserWindow.fromWebContents(event.sender);
			const name = ['file', 'edit', 'view', 'help'].includes(request?.name) ? request.name : null;
			const menu = name ? Menu.getApplicationMenu()?.getMenuItemById(`fella-menu-${name}`)?.submenu : null;
			if (!owner || !menu) return false;
			menu.popup({ window: owner, x: Math.round(request.x), y: Math.round(request.y) });
			return true;
		});
		ipcMain.handle('fella:ask', (event, request) =>
			requestEngine('ask', request.params ?? {}, (item) => {
				if (!event.sender.isDestroyed()) {
					event.sender.send('fella:ask-event', { requestId: request.requestId, event: item });
				}
			})
		);
		ipcMain.handle('fella:analysis-turn-rerun', (event, request) =>
			requestEngine('analysis_turn_rerun', request.params ?? {}, (item) => {
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
		ipcMain.handle('fella:sample-workspace-path', () =>
			app.isPackaged
				? join(process.resourcesPath, 'fella-sample-data')
				: join(here, 'fella-sample-data')
		);
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
	isQuitting = true;
	engine?.dispose();
});
