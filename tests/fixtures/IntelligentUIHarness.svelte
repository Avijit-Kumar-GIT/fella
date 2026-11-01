<script lang="ts">
	import { untrack } from 'svelte';
	import AnalyticalPresentation from '../../src/lib/components/presentation/AnalyticalPresentation.svelte';
	import type { BoundResult, PresentationAction, PresentationSpec, PresentationState } from '../../src/lib/presentation/types';
	import '../../src/app.css';
	import '../../src/lib/components/presentation/presentation-theme.css';

	let { spec, companionSpec, results, savedViewerState, onstatechange, onaction, onrequery }: {
		spec: PresentationSpec;
		companionSpec: PresentationSpec | null;
		results: BoundResult[];
		savedViewerState: PresentationState;
		onstatechange: (state: PresentationState) => void;
		onaction: (action: PresentationAction) => void;
		onrequery: (resultId: string) => Promise<BoundResult>;
	} = $props();
	let viewerState = $state<PresentationState>(untrack(() => savedViewerState));
	function changed(state: PresentationState) {
		viewerState = state;
		onstatechange(state);
	}
</script>

<main aria-label="Frozen analytical presentation" style="max-width: 960px; margin: 32px auto; padding: 24px;">
	<AnalyticalPresentation {spec} {results} initialState={viewerState} onstatechange={changed} {onaction} {onrequery} interactionScope="contract-view" />
	{#if companionSpec}
		<!-- An unmodified second frozen case supplies real display interactions
		     while the FollowUp case stays mounted and receives host state changes. -->
		<section aria-label="Frozen display controls">
			<AnalyticalPresentation spec={companionSpec} {results} initialState={viewerState} onstatechange={changed} {onaction} {onrequery} interactionScope="contract-view" />
		</section>
	{/if}
</main>
