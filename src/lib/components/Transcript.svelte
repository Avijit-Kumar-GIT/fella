<script lang="ts">
	import { tick } from 'svelte';
	import { dispatch, openFolder, openSampleWorkspace, rerunAnalysisTurn } from '$lib/commands';
	import { Button } from '$lib/components/ui/button';
	import { session } from '$lib/session.svelte';
	import { openExternal } from '$lib/ipc';
	import { fadeQuick } from '$lib/motion';
	import { handleScrollRegionKeydown } from '$lib/scroll-region';
	import Icon from './Icon.svelte';
	import Logo from './Logo.svelte';
	import Message from './Message.svelte';
	import RunTimeline from './RunTimeline.svelte';

	// The empty screen adapts to what's already set up, so a non-technical user
	// always sees the one next step rather than a bare "not connected".
	let providerId = $derived(session.settings?.provider ?? 'ollama-cloud');
	let provider = $derived(session.providers.find((p) => p.id === providerId));
	let providerName = $derived(provider?.display ?? providerId);
	let getKeyUrl = $derived(provider?.get_key_url ?? '');

	// session.health starts null until the first probe resolves. Collapsing
	// "haven't checked yet" into "not reachable" (the old `up` alone did
	// this) makes a perfectly healthy, already-configured provider flash the
	// full connect-a-provider panel for the length of one IPC round trip --
	// on every single launch, and again if a folder opens before that first
	// check settles. showSetup should wait for a real answer instead of
	// assuming the worst by default.
	let healthChecked = $derived(session.health !== null);
	let up = $derived(session.health?.reachable === true);
	let rejected = $derived(session.health?.rejected === true);
	let currentModel = $derived(session.settings?.model ?? '');
	let healthModels = $derived(session.health?.models ?? []);
	let hasCredential = $derived(session.settings?.has_credential === true);
	// Connected to a hosted service but no model chosen yet (gateways ship with
	// no default; we don't auto-pick from hundreds).
	let needModelPick = $derived(up && !currentModel);

	// Show the connect panel whenever the user can't actually ask a question.
	let showSetup = $derived(healthChecked && (rejected || !up || needModelPick));
	let services = $derived(session.providers.filter((p) => p.auth === 'key' && p.id !== 'custom'));

	/** Start the in-app key entry flow without navigating away from Fella. */
	function connectService(id: string) {
		void dispatch(`/login ${id}`);
	}

	let hasFolder = $derived(!!session.catalog.workspace);
	let hasConversationTurn = $derived(session.messages.some((message) => message.role !== 'system'));
	let folderName = $derived(
		session.catalog.workspace?.replace(/[/\\]+$/, '').replace(/^.*[/\\]/, '') ?? ''
	);
	let fileCount = $derived(session.catalog.sources.length);
	let skipped = $derived(session.catalog.skipped ?? []);
	let scroller: HTMLDivElement;
	let expanded = $state<Record<string, boolean>>({});

	function toggle(id: string) {
		expanded = { ...expanded, [id]: !expanded[id] };
	}

	function rerun(message: import('$lib/types').Message): Promise<void> {
		return rerunAnalysisTurn(message);
	}

	let stick = $state(true);
	function onScroll() {
		if (!scroller) return;
		stick = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 40;
	}
	// Switching conversations shows a different transcript jump it to the latest and
	// drop any stale expanded-evidence state from the previous one. Entering
	// focus mode collapses evidence too, so it's just the answers.
	$effect(() => {
		session.activeConversationIndex;
		session.focus;
		stick = true;
		expanded = {};
	});
	$effect(() => {
		session.messages.length;
		session.messages.at(-1)?.text;
		session.activeChat?.runSteps.length;
		if (stick && scroller) {
			queueMicrotask(() => scroller.scrollTo({ top: scroller.scrollHeight }));
		}
	});

	$effect(() => {
		const target = session.conversationAnalysisFocus;
		if (!target || target.conversationId !== session.activeChat.id) return;
		void tick().then(() => {
			if (session.conversationAnalysisFocus !== target || target.conversationId !== session.activeChat.id) return;
			const message = [...(scroller?.querySelectorAll<HTMLElement>('[data-analysis-turn-id]') ?? [])]
				.find((element) => element.dataset.analysisTurnId === target.turnId);
			if (!message) return;
			message.scrollIntoView({ behavior: 'smooth', block: 'center' });
			session.clearConversationAnalysisFocus(target);
		});
	});

	export function collapseAll() {
		expanded = {};
	}
</script>

<!-- role="log" for structure, but not a live region the answer streams in
     token by token, which would make a screen reader read every fragment. The
     visually-hidden status line in +page.svelte does the announcing instead. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
	class="transcript"
	class:focus={session.focus}
	bind:this={scroller}
	tabindex="0"
	onkeydown={handleScrollRegionKeydown}
	onscroll={onScroll}
	role="log"
	aria-label="conversation"
	aria-live="off"
>
	{#if !hasConversationTurn}
		<div class="onboard" class:center={!showSetup}>
			<div class="onboard-brand"><Logo size={28} /><span>Fella</span></div>
			<h1 class="hero">New conversation</h1>
			<p class="lead">
				Ask a question about your files. Fella checks what’s available and shows the answer with supporting evidence.
			</p>

			{#if !hasFolder}
				<div class="cta first-run-actions">
					<Button class="folder-action" onclick={() => void openFolder(undefined, { focusAfterOpen: true })}>
						<Icon name="folder" size={16} /> Choose a folder
					</Button>
					<Button variant="outline" onclick={() => void openSampleWorkspace()}>
						Try sample data
					</Button>
				</div>
			{:else if fileCount === 0}
				<p class="onboard-note">No readable files in <strong>{folderName}</strong>.</p>
				<div class="cta first-run-actions">
					<Button class="folder-action" onclick={() => void openFolder()}>
						<Icon name="folder" size={16} /> Choose a different folder
					</Button>
					{#if skipped.length}
						<Button variant="outline" onclick={() => void dispatch('/files')}>Review files</Button>
					{/if}
				</div>
			{:else}
				<!-- A new chat stays quiet and consistent; source details live in Data & context. -->
			{/if}

			{#if !hasFolder}
				<p class="first-run-privacy">
					Fella reads and computes locally. Only the folder you choose is in scope. Your chosen model provider receives prompts and relevant context or tool results.
				</p>
				<div class="first-run-privacy-link">
					<Button variant="link" size="sm" class="privacy-link" onclick={() => void openExternal('https://docs.lilfella.app/developer-platform/using-fella/privacy')}>
						Privacy and security <Icon name="arrow-up-right" size={12} />
					</Button>
				</div>
			{/if}

			{#if showSetup}
				<details class="model-setup" open={hasFolder}>
					<summary>Connect a model</summary>
				<div class="setup">
					{#if rejected}
						<!-- Hosted key was refused (HTTP 401/403). -->
						<p>Your <strong>{providerName}</strong> key was refused.</p>
						<div class="svc">
							<Button variant="outline" size="sm" onclick={() => void dispatch(`/login ${providerId}`)}>Enter a new key</Button>
							{#if getKeyUrl}
								<Button variant="ghost" size="sm" onclick={() => void openExternal(getKeyUrl)}>
									Get a new key <Icon name="arrow-up-right" size={12} />
								</Button>
							{/if}
						</div>
					{:else if needModelPick}
						<!-- Connected to a hosted service, no model chosen. -->
						<p>Connected to <strong>{providerName}</strong>. Choose a model to finish.</p>
						{#if healthModels.length && healthModels.length <= 12}
							<div class="svc">
								{#each healthModels as m (m)}
									<Button variant="outline" size="sm" onclick={() => void dispatch(`/model ${m}`)}>{m}</Button>
								{/each}
							</div>
						{:else}
							<p class="alt">Run <code>/model</code> to see what's available and pick one.</p>
						{/if}
					{:else if !hasCredential}
						<p>Connect a model service to start asking questions about your files.</p>
						{#if services.length}
							<div class="svc">
								{#each services as p (p.id)}
									<Button variant="outline" size="sm" onclick={() => connectService(p.id)}>{p.display}</Button>
								{/each}
							</div>
						{:else}
							<p class="alt">Run <code>/login</code> to connect a provider with your API key.</p>
						{/if}
					{:else}
						<p>Can't reach <strong>{providerName}</strong>. Check your internet connection or enter a new key.</p>
						<div class="svc">
							<Button variant="outline" size="sm" onclick={() => void dispatch(`/login ${providerId}`)}>Check key</Button>
							{#each services.filter((p) => p.id !== providerId) as p (p.id)}
								<Button variant="ghost" size="sm" onclick={() => connectService(p.id)}>{p.display}</Button>
							{/each}
						</div>
					{/if}
				</div>
				</details>
			{/if}

		</div>
	{/if}
	{#if session.messages.length > 0}
		{#if showSetup && hasConversationTurn}
			<!-- The full onboarding panel only shows on the empty screen. If the
			     model stops working mid-session, keep a compact version in view so
			     the user isn't left with just a red dot and bare error lines. -->
			<div class="setup compact stream">
				{#if rejected}
					<p>
						Your <strong>{providerName}</strong> key was refused.
						<Button variant="link" size="sm" class="setup-inline-link" onclick={() => void dispatch(`/login ${providerId}`)}>
							Enter a new key
						</Button>
					</p>
				{:else if needModelPick}
					<p>
						Connected to <strong>{providerName}</strong>, but no model is chosen.
						<Button variant="link" size="sm" class="setup-inline-link" onclick={() => void dispatch('/model')}>Pick a model</Button>
					</p>
				{:else if !hasCredential}
					<p>
						No model service is connected.
						<Button variant="link" size="sm" class="setup-inline-link" onclick={() => void dispatch('/login')}>Connect one with an API key</Button>.
					</p>
				{:else}
					<p>
						Can't reach <strong>{providerName}</strong>.
						<Button variant="link" size="sm" class="setup-inline-link" onclick={() => void dispatch(`/login ${providerId}`)}>Check the key</Button>
						or try another provider.
					</p>
				{/if}
			</div>
		{/if}
		{#key session.activeConversationIndex}
			<div class="stream" in:fadeQuick>
				<svelte:boundary>
					{#each session.messages as m, i (m.id)}
						<Message
							message={m}
							expanded={!!expanded[m.id]}
							ontoggle={() => toggle(m.id)}
							onopeninvestigationrun={session.activeChat.workspaceScope === session.catalog.workspace
								? (turnId) => session.openInvestigationRun(turnId)
								: undefined}
							onopenchart={(evidenceIndex) => session.openChartPane(m.id, evidenceIndex)}
							openChartIndex={session.activeChat.companionPane?.kind === 'chart' && session.activeChat.companionPane.messageId === m.id
								? session.activeChat.companionPane.evidenceIndex
								: null}
							onrerun={() => rerun(m)}
						/>
						{#if m.role === 'assistant' && (m.pending || m.answer?.trace?.steps.length || m.run_steps?.length)}
							<RunTimeline
								message={m}
								bodyId={`evidence-${m.id}`}
								onopenevidence={() => {
									if (!expanded[m.id]) toggle(m.id);
								}}
							/>
						{/if}
					{/each}
					{#snippet failed(error)}
						<pre class="boundary-err">The transcript hit a render error:
{String(error)}</pre>
					{/snippet}
				</svelte:boundary>
			</div>
		{/key}
	{/if}
</div>

{#if !stick && hasConversationTurn}
	<button class="jump-latest" onclick={() => { stick = true; scroller?.scrollTo({ top:scroller.scrollHeight, behavior:'smooth' }); }}><Icon name="arrow-down" size={11} />Jump to latest</button>
{/if}

<style>
	.jump-latest { position:absolute; z-index:10; bottom:80px; left:50%; display:flex; align-items:center; gap:6px; transform:translateX(-50%); padding:4px 12px; border:0; border-radius:999px; background:var(--brand); color:white; box-shadow:var(--shadow-sm); font-size:12px; cursor:pointer; }
	.model-setup { margin-top: 20px; color: var(--text-dim); font-size: var(--fs-sm); }
	.model-setup > summary { cursor: pointer; }
	.transcript {
		flex: 1;
		overflow-y: auto;
		overscroll-behavior: contain;
		padding:56px 20px 24px;
		min-height: 0;
		transition: padding var(--dur) var(--ease);
	}
	/* Focus mode: a calmer, tighter reading column with more air around it. */
	.transcript.focus {
		padding: var(--space-6) var(--pad) var(--space-6);
	}
	/* Cap the reading column so long lines don't sprawl across the app
	   cue. Centred in the scroller. */
	.stream {
		width:100%;
		max-width:520px;
		margin-inline:auto;
		font-size:13px;
		line-height:21px;
		transition: max-width var(--dur) var(--ease);
	}
	.transcript.focus .stream {
		max-width: 68ch;
	}
	.onboard {
		max-width: 56ch;
		margin: var(--space-6) auto 0;
		color: var(--text-dim);
	}
	/* First run, nothing to set up: a centred hero rather than a top-aligned
	   wall. Drops back to top-aligned as soon as the setup card appears. */
	.onboard.center {
		min-height: 100%;
		margin: 0 auto;
		display: flex;
		flex-direction: column;
		justify-content: center;
		align-items: center;
		text-align: center;
	}
	.onboard-brand {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: var(--space-2);
		margin-bottom: var(--space-3);
		color: var(--text-dim);
		font-size: var(--fs-sm);
		font-weight: 650;
		letter-spacing: -0.01em;
	}
	.hero {
		font-size: clamp(26px, 3vw, 34px);
		font-weight: 650;
		letter-spacing: -0.035em;
		text-wrap: balance;
		color: var(--text);
		margin: 0 0 var(--space-3);
	}
	.onboard p {
		margin: 0 0 var(--space-3);
	}
	.onboard .lead {
		color: var(--text);
		font-size: 17px;
		line-height: 1.55;
	}
	.onboard-note {
		color: var(--text-faint);
		font-size: var(--fs-sm);
	}
	.first-run-actions {
		justify-content: center;
	}
	.first-run-privacy {
		max-width: 58ch;
		margin: var(--space-4) 0 0 !important;
		color: var(--text-faint);
		font-size: var(--fs-xs);
		line-height: 1.55;
	}
	.first-run-privacy-link { margin-top: var(--space-1); }
	.first-run-privacy :global(.privacy-link) {
		display: inline;
		min-height: 0;
		height: auto;
		padding: 0;
		font-size: inherit;
		vertical-align: baseline;
	}
	.cta {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-3);
		margin: var(--space-5) 0 var(--space-2);
	}
	.onboard.center .cta {
		justify-content: center;
	}
	/* A hairline, not a boxed card echoes the divider language used
	   elsewhere (titlebar/sidebar) rather than looking like a bolted-on form. */
	.setup {
		margin-top: var(--space-6);
		padding-top: var(--space-4);
		border-top: 1px solid var(--border);
	}
	.setup p {
		margin: 0 0 var(--space-3);
	}
	/* Mid-session health banner: not the full first-run panel, just enough to
	   point at the fix. Sits above the transcript. */
	.setup.compact {
		margin: 0 auto var(--space-4);
		padding: var(--space-3);
		border: 1px solid var(--border);
		border-left: 2px solid var(--warn);
		box-shadow: none;
		background: var(--bg-inset);
		color: var(--text-dim);
		font-size: var(--fs-sm);
	}
	.setup.compact p {
		margin: 0;
	}
	.setup .alt {
		color: var(--text-faint);
	}
	.setup code {
		font-family: var(--mono);
		font-size: 0.92em;
		background: var(--bg-inset);
		border-radius: var(--radius-chip);
		padding: 1px 5px;
	}
	.svc {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
		margin-top: 4px;
	}
	/* Shared link buttons remain inline in compact, sentence-level guidance. */
	.setup :global(.setup-inline-link) {
		display: inline;
		min-width: 0;
		height: auto;
		min-height: 0;
		padding: 0;
		border-radius: 0;
		font-size: inherit;
		font-weight: 600;
		line-height: inherit;
		vertical-align: baseline;
		white-space: normal;
	}
	.boundary-err {
		font-family: var(--mono);
		font-size: var(--fs-sm);
		color: var(--warn);
		white-space: pre-wrap;
		word-break: break-word;
	}
</style>
