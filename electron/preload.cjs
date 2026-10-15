const { contextBridge, ipcRenderer, webUtils } = require('electron');

const listeners = new Map();
const terminalListeners = new Map();
let nextAskId = 1;
let nextTerminalId = 1;

ipcRenderer.on('fella:ask-event', (_event, message) => {
	listeners.get(message.requestId)?.(message.event);
});
ipcRenderer.on('fella:workspace-progress', (_event, message) => {
	listeners.get(message.requestId)?.(message.event);
});
ipcRenderer.on('fella:terminal-data', (_event, message) => {
	terminalListeners.get(message.terminalId)?.onData(message.data);
});
ipcRenderer.on('fella:terminal-exit', (_event, message) => {
	const callbacks = terminalListeners.get(message.terminalId);
	terminalListeners.delete(message.terminalId);
	callbacks?.onExit({ exitCode: message.exitCode, signal: message.signal });
});

contextBridge.exposeInMainWorld('fella', {
	invoke(command, args) {
		return ipcRenderer.invoke('fella:invoke', { command, args });
	},

	openWorkspace(path, onProgress) {
		const requestId = `mount-${nextAskId++}`;
		listeners.set(requestId, onProgress);
		return ipcRenderer
			.invoke('fella:open-workspace', { requestId, path })
			.finally(() => listeners.delete(requestId));
	},

	listTerminalShells() {
		return ipcRenderer.invoke('fella:terminal-shells');
	},

	startTerminal(workspacePath, shellPath, cols, rows, onData, onExit) {
		const terminalId = `terminal-${nextTerminalId++}`;
		terminalListeners.set(terminalId, { onData, onExit });
		return ipcRenderer.invoke('fella:terminal-start', { terminalId, workspacePath, shellPath, cols, rows }).catch((error) => {
			terminalListeners.delete(terminalId);
			throw error;
		});
	},

	writeTerminal(terminalId, data) {
		ipcRenderer.send('fella:terminal-input', { terminalId, data });
	},

	resizeTerminal(terminalId, cols, rows) {
		ipcRenderer.send('fella:terminal-resize', { terminalId, cols, rows });
	},

	closeTerminal(terminalId) {
		terminalListeners.delete(terminalId);
		ipcRenderer.send('fella:terminal-close', terminalId);
	},

	ask(params, onEvent) {
		const requestId = `ask-${nextAskId++}`;
		listeners.set(requestId, onEvent);
		return ipcRenderer
			.invoke('fella:ask', { requestId, params })
			.finally(() => listeners.delete(requestId));
	},

	rerunAnalysisTurn(params, onEvent) {
		const requestId = `rerun-${nextAskId++}`;
		listeners.set(requestId, onEvent);
		return ipcRenderer
			.invoke('fella:analysis-turn-rerun', { requestId, params })
			.finally(() => listeners.delete(requestId));
	},

	pickFolder() {
		return ipcRenderer.invoke('fella:pick-folder');
	},

	sampleWorkspacePath() {
		return ipcRenderer.invoke('fella:sample-workspace-path');
	},

	openExternal(url) {
		return ipcRenderer.invoke('fella:open-external', url);
	},

	setWindowAppearance(dark) {
		return ipcRenderer.invoke('fella:set-window-appearance', dark);
	},

	pathForFile(file) {
		return webUtils.getPathForFile(file);
	},

	windowAction(action) {
		return ipcRenderer.invoke('fella:window-action', action);
	}
});
