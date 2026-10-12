/** Keep a focused, overflowed region navigable with standard paging keys. */
export function handleScrollRegionKeydown(event: KeyboardEvent): void {
	if (
		event.defaultPrevented ||
		event.target !== event.currentTarget ||
		event.altKey ||
		event.ctrlKey ||
		event.metaKey ||
		event.shiftKey
	) return;

	const region = event.currentTarget;
	if (!(region instanceof HTMLElement)) return;

	let nextTop = region.scrollTop;
	let nextLeft = region.scrollLeft;
	switch (event.key) {
		case 'Home':
			nextTop = 0;
			break;
		case 'End':
			nextTop = region.scrollHeight;
			break;
		case 'PageUp':
			nextTop = region.scrollTop - region.clientHeight;
			break;
		case 'PageDown':
			nextTop = region.scrollTop + region.clientHeight;
			break;
		case 'ArrowUp':
			nextTop -= 40;
			break;
		case 'ArrowDown':
			nextTop += 40;
			break;
		case 'ArrowLeft':
			nextLeft -= 40;
			break;
		case 'ArrowRight':
			nextLeft += 40;
			break;
		default:
			return;
	}

	if (nextTop === region.scrollTop && nextLeft === region.scrollLeft) return;
	event.preventDefault();
	region.scrollTop = nextTop;
	region.scrollLeft = nextLeft;
}
