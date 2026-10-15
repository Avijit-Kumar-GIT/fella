/**
 * Format PTY output for Fella's terminal without GNU ls's green writable-
 * directory background blocks. Keep the file's foreground color and all
 * unrelated ANSI styling intact.
 *
 * The formatter carries partial CSI sequences across PTY chunks, since a
 * control sequence can be split at any byte boundary by the PTY reader.
 */
export function createTerminalOutputFormatter() {
	let pending = '';

	/** @param {string} chunk */
	return function formatTerminalOutput(chunk) {
		const input = pending + chunk;
		pending = '';
		let output = '';
		let cursor = 0;

		while (cursor < input.length) {
			const escape = input.indexOf('\u001b', cursor);
			if (escape === -1) {
				output += input.slice(cursor);
				break;
			}

			output += input.slice(cursor, escape);
			if (escape + 1 >= input.length) {
				pending = input.slice(escape);
				break;
			}

			if (input[escape + 1] !== '[') {
				output += input[escape];
				cursor = escape + 1;
				continue;
			}

			let end = escape + 2;
			while (end < input.length) {
				const code = input.charCodeAt(end);
				if (code >= 0x40 && code <= 0x7e) break;
				end += 1;
			}

			if (end === input.length) {
				if (input.length - escape <= 128) {
					pending = input.slice(escape);
					break;
				}
				output += input.slice(escape, escape + 2);
				cursor = escape + 2;
				continue;
			}

			const sequence = input.slice(escape, end + 1);
			output += sequence.endsWith('m') ? withoutGreenBackground(sequence) : sequence;
			cursor = end + 1;
		}

		return output;
	};
}

/** @param {string} sequence */
function withoutGreenBackground(sequence) {
	const parameters = sequence.slice(2, -1).split(';');
	const normalized = [];

	for (let index = 0; index < parameters.length; index += 1) {
		const parameter = parameters[index];
		if (parameter === '38' && parameters[index + 1] === '5') {
			normalized.push(...parameters.slice(index, index + 3));
			index += 2;
			continue;
		}
		if (parameter === '38' && parameters[index + 1] === '2') {
			normalized.push(...parameters.slice(index, index + 5));
			index += 4;
			continue;
		}

		if (parameter === '42' || parameter === '102') {
			normalized.push('49');
			continue;
		}

		// Also handle indexed SGR colors commonly used by dircolors.
		if (
			parameter === '48' && parameters[index + 1] === '5' &&
			(parameters[index + 2] === '2' || parameters[index + 2] === '10')
		) {
			normalized.push('49');
			index += 2;
			continue;
		}
		if (parameter === '48' && parameters[index + 1] === '5') {
			normalized.push(...parameters.slice(index, index + 3));
			index += 2;
			continue;
		}
		if (parameter === '48' && parameters[index + 1] === '2') {
			normalized.push(...parameters.slice(index, index + 5));
			index += 4;
			continue;
		}

		normalized.push(parameter);
	}

	return `\u001b[${normalized.join(';')}m`;
}
