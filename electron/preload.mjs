import { contextBridge, ipcRenderer, webUtils } from 'electron';

const listeners = new Map();
let nextAskId = 1;

ipcRenderer.on('fella:ask-event', (_event, message) => {
	listeners.get(message.requestId)?.(message.event);
});

contextBridge.exposeInMainWorld('fella', {
	invoke(command, args) {
		return ipcRenderer.invoke('fella:invoke', { command, args });
	},

	ask(params, onEvent) {
		const requestId = `ask-${nextAskId++}`;
		listeners.set(requestId, onEvent);
		return ipcRenderer
			.invoke('fella:ask', { requestId, params })
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
