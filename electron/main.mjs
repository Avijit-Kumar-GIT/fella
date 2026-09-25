import { app, BrowserWindow, dialog, ipcMain, net, protocol, shell } from 'electron';
import { existsSync, mkdirSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { EngineClient, assertBinary } from './engine.mjs';

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

function dataDirectory() {
	const configured = process.env.FELLA_DATA_DIR?.trim();
	if (configured) return resolve(configured);
	// Keep the experiment's data separate from the Tauri install while both
	// branches may be open during a comparison.
	return join(app.getPath('appData'), 'dev.fella.app-electron');
}

function createWindow() {
	const mac = process.platform === 'darwin';
	const win = new BrowserWindow({
		width: 1280,
		height: 800,
		minWidth: 960,
		minHeight: 640,
		show: false,
		frame: mac,
		titleBarStyle: mac ? 'hiddenInset' : undefined,
		backgroundColor: '#0e0e10',
		autoHideMenuBar: true,
		webPreferences: {
			preload: join(here, 'preload.mjs'),
			contextIsolation: true,
			nodeIntegration: false,
			sandbox: true
		}
	});

	win.once('ready-to-show', () => win.show());
	win.webContents.setWindowOpenHandler(({ url }) => {
		void openExternal(url);
		return { action: 'deny' };
	});

	const devUrl = process.env.FELLA_ELECTRON_URL;
	const load = devUrl ? win.loadURL(devUrl) : win.loadURL('fella://app/index.html');
	load.catch((error) => {
		console.error('Fella window failed to load:', error);
		void dialog.showErrorBox('Fella could not start', String(error));
	});
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
	const buildRoot = resolve(app.getAppPath(), 'build');
	protocol.handle('fella', (request) => {
		const pathname = decodeURIComponent(new URL(request.url).pathname);
		const target = resolve(buildRoot, `.${pathname || '/index.html'}`);
		const allowed = target === buildRoot || relative(buildRoot, target).split(sep)[0] !== '..';
		if (!allowed) return new Response('Forbidden', { status: 403 });
		return net.fetch(pathToFileURL(target).toString());
	});

		const binary = findEngine();
		try {
			assertBinary(binary);
			const dataDir = dataDirectory();
			mkdirSync(dataDir, { recursive: true });
			engine = new EngineClient(binary, dataDir);
		} catch (error) {
			void dialog.showErrorBox('Fella engine unavailable', String(error));
			app.quit();
			return;
		}

		ipcMain.handle('fella:invoke', (_event, request) =>
			engine.request(request.command, request.args ?? {})
		);
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

		createWindow();
	});

app.on('window-all-closed', () => {
		if (process.platform !== 'darwin') app.quit();
	});

app.on('before-quit', () => {
		engine?.dispose();
	});
