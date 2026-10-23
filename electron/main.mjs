import { app, BrowserWindow, dialog, ipcMain, Menu, nativeTheme, protocol, screen, shell } from 'electron';
import { mkdirSync, writeFileSync } from 'node:fs';
import { readFile, realpath, stat } from 'node:fs/promises';
import { homedir } from 'node:os';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { EngineClient, assertBinary, isRegularFile } from './engine.mjs';
import { EmbeddedTerminalSessions } from './embedded-terminal.mjs';
import { discoverTerminalShells } from './terminal-shells.mjs';
import { checkAndApply } from './update.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..');
let engine;
let terminalSessions;
app.setName('Fella');

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
		label: 'Edit',
		submenu: [
			{ role: 'undo' }, { role: 'redo' }, { type: 'separator' },
			{ role: 'cut' }, { role: 'copy' }, { role: 'paste' }, { role: 'selectAll' }
		]
	});
	menus.push({
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
		role: 'help',
		submenu: [{ label: 'Fella on GitHub', click: () => void openExternal('https://github.com/Avijit-Kumar-GIT/fella') }]
	});
	Menu.setApplicationMenu(Menu.buildFromTemplate(menus));
}

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
	const savedState = await readWindowState();
	const win = new BrowserWindow({
		...(savedState?.bounds ?? { width: 1280, height: 800 }),
		minWidth: 560,
		minHeight: 640,
		resizable: true,
		show: false,
		// Windows is frameless because Fella draws its own controls; Linux and
		// macOS retain the native frame.
		frame: !windows,
		titleBarStyle: mac ? 'hiddenInset' : undefined,
		backgroundColor: nativeTheme.shouldUseDarkColors ? DARK_WINDOW : LIGHT_WINDOW,
		autoHideMenuBar: false,
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
	win.webContents.on('did-start-navigation', (_event, details) => {
		if (details.isMainFrame) terminalSessions.closeForOwner(win.webContents);
	});
	win.webContents.on('render-process-gone', () => terminalSessions.closeForOwner(win.webContents));
	win.webContents.once('destroyed', () => terminalSessions.closeForOwner(win.webContents));

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
			terminalSessions = new EmbeddedTerminalSessions({ binary });
		} catch (error) {
			void dialog.showErrorBox('Fella engine unavailable', String(error));
			app.quit();
			return;
		}

		ipcMain.handle('fella:invoke', (_event, request) => {
			if (request.command === 'update') return checkAndApply(app);
			return engine.request(request.command, request.args ?? {});
		});
		ipcMain.handle('fella:open-workspace', (event, request) =>
			engine.request('open_workspace', { path: request.path }, (item) => {
				if (!event.sender.isDestroyed()) {
					event.sender.send('fella:workspace-progress', {
						requestId: request.requestId,
						event: item
					});
				}
			})
		);
		ipcMain.handle('fella:terminal-shells', (event) => {
			if (!isTrustedAppFrame(event)) throw new Error('Shell choices are only available from Fella.');
			return discoverTerminalShells();
		});
		ipcMain.handle('fella:terminal-start', async (event, request) => {
			if (!isTrustedAppFrame(event)) throw new Error('Shell sessions are only available from Fella.');
			if (typeof request?.workspacePath !== 'string' || !request.workspacePath.trim()) {
				throw new Error('Choose a workspace before opening a shell.');
			}
			const canonicalPath = await realpath(request.workspacePath);
			if (!(await stat(canonicalPath)).isDirectory()) {
				throw new Error('The selected workspace is no longer an available folder.');
			}
			const availableShells = discoverTerminalShells().shells;
			if (!availableShells.some((item) => item.path === request?.shellPath)) {
				throw new Error('That shell is no longer available. Choose an installed shell and try again.');
			}
			return terminalSessions.start(event.sender, {
				...request,
				workspacePath: canonicalPath
			});
		});
		ipcMain.on('fella:terminal-input', (event, request) => {
			if (!isTrustedAppFrame(event)) return;
			terminalSessions.write(event.sender, request?.terminalId, request?.data);
		});
		ipcMain.on('fella:terminal-resize', (event, request) => {
			if (!isTrustedAppFrame(event)) return;
			terminalSessions.resize(event.sender, request?.terminalId, request?.cols, request?.rows);
		});
		ipcMain.on('fella:terminal-close', (event, terminalId) => {
			if (!isTrustedAppFrame(event)) return;
			terminalSessions.close(event.sender, terminalId);
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
		ipcMain.handle('fella:analysis-turn-rerun', (event, request) =>
			engine.request('analysis_turn_rerun', request.params ?? {}, (item) => {
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
	terminalSessions?.dispose();
	engine?.dispose();
});
