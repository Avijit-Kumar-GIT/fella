<script lang="ts">
	import { prefs } from '$lib/prefs.svelte';

	type ProviderAsset = {
		light: string;
		dark?: string;
		/** Some official marks only ship in one contrast; adapt it for the other. */
		invertLight?: boolean;
		invertDark?: boolean;
	};

	const PROVIDER_ASSETS: Record<string, ProviderAsset> = {
		openai: { light: '/brand/providers/openai.svg', invertDark: true },
		vercel: {
			light: '/brand/providers/vercel-light.svg',
			dark: '/brand/providers/vercel-dark.svg'
		},
		xai: { light: '/brand/providers/xai.ico', invertLight: true },
		'ollama-cloud': { light: '/brand/providers/ollama.svg', invertDark: true },
		openrouter: { light: '/brand/providers/openrouter.svg' }
	};

	let { providerId, size = 13 }: { providerId: string; size?: number } = $props();
	let asset = $derived(PROVIDER_ASSETS[providerId.toLowerCase()]);
	let src = $derived(prefs.isDark && asset?.dark ? asset.dark : asset?.light ?? '');
	let invert = $derived(
		prefs.isDark
			? !asset?.dark && asset?.invertDark === true
			: asset?.invertLight === true
	);
</script>

{#if asset}
	<span class="provider-icon" style={`width:${size}px;height:${size}px;`}>
		<img
			class:invert={invert}
			src={src}
			width={size}
			height={size}
			alt=""
			aria-hidden="true"
			decoding="async"
		/>
	</span>
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
	.provider-icon img.invert {
		filter: invert(1);
	}
</style>
