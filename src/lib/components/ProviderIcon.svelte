<script lang="ts">
	type ProviderAsset = {
		light: string;
		dark?: string;
	};

	const PROVIDER_ASSETS: Record<string, ProviderAsset> = {
		openai: { light: '/brand/providers/openai.svg' },
		vercel: {
			light: '/brand/providers/vercel-light.svg',
			dark: '/brand/providers/vercel-dark.svg'
		},
		xai: { light: '/brand/providers/xai.ico' },
		'ollama-cloud': { light: '/brand/providers/ollama.svg' },
		ollama: { light: '/brand/providers/ollama.svg' },
		openrouter: { light: '/brand/providers/openrouter.svg' }
	};

	let { providerId, size = 13 }: { providerId: string; size?: number } = $props();
	let asset = $derived(PROVIDER_ASSETS[providerId.toLowerCase()]);
</script>

{#if asset}
	<picture class="provider-icon" style={`width:${size}px;height:${size}px;`}>
		{#if asset.dark}<source media="(prefers-color-scheme: dark)" srcset={asset.dark} />{/if}
		<img src={asset.light} width={size} height={size} alt="" aria-hidden="true" decoding="async" />
	</picture>
{/if}

<style>
	.provider-icon {
		display: inline-flex;
		flex: none;
		vertical-align: middle;
	}
	.provider-icon img {
		display: block;
		width: 100%;
		height: 100%;
		object-fit: contain;
	}
</style>
