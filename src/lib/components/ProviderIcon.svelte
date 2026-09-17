<script lang="ts">
	let { providerId, size = 13 }: { providerId: string; size?: number } = $props();

	let provider = $derived(providerId.toLowerCase());
</script>

<svg
	class="provider-icon"
	width={size}
	height={size}
	viewBox="0 0 24 24"
	fill="none"
	stroke="currentColor"
	stroke-width="1.8"
	stroke-linecap="round"
	stroke-linejoin="round"
	aria-hidden="true"
	focusable="false"
>
	{#if provider === 'openai'}
		<path d="M12 3.5a4.5 4.5 0 0 1 4.2 2.9A4.5 4.5 0 0 1 20.5 11a4.5 4.5 0 0 1-2.9 4.2A4.5 4.5 0 0 1 14 19.5a4.5 4.5 0 0 1-4.2-2.9A4.5 4.5 0 0 1 3.5 13a4.5 4.5 0 0 1 2.9-4.2A4.5 4.5 0 0 1 12 3.5Z" />
		<path d="m8.2 10.2 3.8-2.2 3.8 2.2v4.4L12 16.8l-3.8-2.2v-4.4Z" />
	{:else if provider === 'vercel'}
		<path d="M12 4 21 20H3Z" fill="currentColor" stroke="none" />
	{:else if provider === 'xai'}
		<path d="m5 5 14 14M19 5 5 19" />
	{:else if provider === 'ollama' || provider === 'ollama-cloud'}
		<circle cx="12" cy="12" r="7.5" />
		<path d="M8.5 10.5h.01M15.5 10.5h.01M9 15c1.8 1.2 4.2 1.2 6 0" />
	{:else if provider === 'openrouter'}
		<path d="M4 8h8a4 4 0 0 1 4 4v4M20 16l-4 4-4-4M4 16h4" />
	{:else if provider === 'custom'}
		<path d="m12 3 9 9-9 9-9-9 9-9Z" />
		<path d="m9 12 2 2 4-4" />
	{:else}
		<path d="M12 3.5 15 6l3.5-.5.5 3.5 2.5 3-2.5 3 .5 3.5-3.5-.5-3 2.5-3-2.5-3.5.5.5-3.5-2.5-3 2.5-3-.5-3.5L9 6l3-2.5Z" />
		<path d="m8.5 12 2.2 2.2 4.8-4.8" />
	{/if}
</svg>

<style>
	.provider-icon {
		display: inline-block;
		flex: none;
		vertical-align: middle;
	}
</style>
