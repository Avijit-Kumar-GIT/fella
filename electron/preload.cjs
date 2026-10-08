const { contextBridge, ipcRenderer, webUtils } = require('electron');

const listeners = new Map();
let nextAskId = 1;

ipcRenderer.on('fella:ask-event', (_event, message) => {
	listeners.get(message.requestId)?.(message.event);
});
ipcRenderer.on('fella:workspace-progress', (_event, message) => {
	listeners.get(message.requestId)?.(message.event);
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
