<script lang="ts">
	import type { BaseComponentProps } from '@json-render/svelte';
	import MarkdownContent from '../MarkdownContent.svelte';
	let { props }: BaseComponentProps<{ content:string }> = $props();
</script>
<div class="presentation-text"><MarkdownContent text={props.content} /></div>
<style>
	.presentation-text { min-width:0; color:var(--chat-body); font-size:var(--fs); line-height:21px; overflow-wrap:anywhere; }
	.presentation-text :global(strong) { color:var(--text); font-weight:600; }
</style>
