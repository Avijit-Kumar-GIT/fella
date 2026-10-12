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
	let showExamples = $derived(up && !!currentModel && !needModelPick);

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
	let sampleWorkspace = $derived(
		session.catalog.workspace?.replace(/[/\\]+$/, '').toLowerCase().endsWith('fella-sample-data') ?? false
	);

	const OVERVIEW_PROMPT = 'Summarize these files and suggest questions I can answer.';
	const GENERAL_EXAMPLES = [
		OVERVIEW_PROMPT,
		'What changed over time in these files?',
		'Compare the main categories in these files.'
	];
	const SAMPLE_EXAMPLES = [
		OVERVIEW_PROMPT,
		'Which channel had the largest absolute increase in signups from January to December 2025?',
		'How did marketing spend change alongside signups?'
	];
	let examples = $derived(sampleWorkspace ? SAMPLE_EXAMPLES : GENERAL_EXAMPLES);

	let scroller: HTMLDivElement;
	let expanded = $state<Record<string, boolean>>({});

	function toggle(id: string) {
		expanded = { ...expanded, [id]: !expanded[id] };
	}

	function questionFor(index: number): string {
		for (let i = index - 1; i >= 0; i--) {
			if (session.messages[i]?.role === 'user') return session.messages[i].text;
		}
		return '';
	}

	function rerun(message: import('$lib/types').Message): Promise<void> {
		return rerunAnalysisTurn(message);
	}

	let stick = true;
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
		<div class="onboard" class:center={!hasFolder && !showSetup}>
			<div class="onboard-brand"><Logo size={28} /><span>Fella</span></div>
			<h1 class="hero">
				{!hasFolder ? 'New conversation' : fileCount ? 'What should we find out?' : 'No readable files yet.'}
			</h1>

			{#if !hasFolder}
				<p class="lead">
					Ask a question about your files. Fella explores the available data, analyzes it, and shows the result with supporting evidence.
				</p>
				<div class="cta first-run-actions">
					<Button class="folder-action" onclick={() => void openFolder(undefined, { focusAfterOpen: true })}>
						<Icon name="folder" size={16} /> Choose a folder
					</Button>
					<Button variant="outline" onclick={() => void openSampleWorkspace()}>
						Try sample data
					</Button>
				</div>
				<p class="first-run-privacy">
					Fella reads and computes locally. Only the folder you choose is in scope. Your chosen model provider receives prompts and relevant context or tool results.
				</p>
				<div class="first-run-privacy-link">
					<Button variant="link" size="sm" class="privacy-link" onclick={() => void openExternal('https://docs.lilfella.app/developer-platform/using-fella/privacy')}>
						Privacy and security <Icon name="arrow-up-right" size={12} />
					</Button>
				</div>
			{:else if fileCount === 0}
				<p class="lead"><strong>{folderName}</strong> is open, but nothing in it is readable yet.</p>
				<p>
					Fella can read spreadsheets, CSV files, PDFs, and plain text.{#if skipped.length} Review the skipped files to see why they could not be loaded.{/if}
				</p>
				<div class="cta">
					<Button class="folder-action" onclick={() => void openFolder()}>
						<Icon name="folder" size={16} /> Choose a different folder
					</Button>
					{#if skipped.length}
						<Button variant="link" onclick={() => void dispatch('/files')}>Review files</Button>
					{/if}
				</div>
			{:else}
				<p class="lead">
					<strong>{folderName}</strong> · {fileCount} readable source{fileCount === 1 ? '' : 's'} available.
				</p>
				{#if showExamples}
					<p class="egs">Not sure what to ask? Start here:</p>
					<div class="examples">
						{#each examples as q (q)}
							<Button variant="outline" class="example" onclick={() => void dispatch(q)}>{q}</Button>
						{/each}
					</div>
				{/if}
			{/if}

			{#if hasFolder}
				<section class="file-inventory" aria-label="File inventory">
					{#if fileCount}
						<p class="inventory-label">Available sources</p>
						<ul>
							{#each session.catalog.sources.slice(0, 3) as source (`available:${source.path}:${source.name}`)}
								<li><span>{source.name}</span></li>
							{/each}
						</ul>
					{/if}
					{#if skipped.length}
						<p class="inventory-label">Not loaded · {skipped.length}</p>
						<ul class="inventory-skipped">
							{#each skipped.slice(0, 3) as file (`skipped:${file.name}`)}
								<li><span>{file.name}</span><span>{file.reason}</span></li>
							{/each}
						</ul>
					{/if}
					{#if fileCount > 3 || skipped.length > 3}
						<Button variant="link" size="sm" class="inventory-more" onclick={() => void dispatch('/files')}>Review all files</Button>
					{/if}
				</section>
			{/if}

			{#if showSetup}
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
			{/if}

			{#if hasFolder && showExamples}
				<p class="personalize">
					Make it yours: <code>/context</code> to tell Fella how your files are organised.
				</p>
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
							question={questionFor(i)}
							showFollowups={i === session.messages.length - 1 && !m.pending}
							openChartIndex={
								session.activeChat.companionPane?.kind === 'chart' &&
								session.activeChat.companionPane.messageId === m.id
									? session.activeChat.companionPane.evidenceIndex
									: null
							}
							onopenchart={(messageId, evidenceIndex) => session.openChartPane(messageId, evidenceIndex)}
							onopeninvestigationrun={session.activeChat.workspaceScope === session.catalog.workspace
								? (turnId) => session.openInvestigationRun(turnId)
								: undefined}
							onfollowup={(next, clarificationTurnId) =>
								void dispatch(next, clarificationTurnId)}
							onrerun={() => rerun(m)}
						/>
						{#if m.role === 'assistant' && (m.pending || m.answer?.trace?.steps.length || m.run_steps?.length)}
							<RunTimeline
								message={m}
								question={questionFor(i)}
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

<style>
	.transcript {
		flex: 1;
		overflow-y: auto;
		overscroll-behavior: contain;
		padding: var(--space-6) var(--pad) var(--space-6);
		min-height: 0;
		transition: padding var(--dur) var(--ease);
	}
	/* Focus mode: a calmer, tighter reading column with more air around it. */
	.transcript.focus {
		padding: var(--space-6) var(--pad) var(--space-6);
	}
	/* Cap the reading column so long lines don't sprawl the "app not terminal"
	   cue. Centred in the scroller. */
	.stream {
		max-width: 76ch;
		margin-inline: auto;
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
	.onboard .lead strong {
		font-weight: 600;
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
	.file-inventory {
		width: 100%;
		margin: var(--space-4) 0 var(--space-2);
		padding-top: var(--space-3);
		border-top: 1px solid var(--border);
		text-align: left;
	}
	.inventory-label {
		margin: 0 0 var(--space-1) !important;
		color: var(--text-faint);
		font-size: var(--fs-xs);
		font-weight: 600;
	}
	.file-inventory ul {
		margin: 0 0 var(--space-3);
		padding: 0;
		list-style: none;
		color: var(--text-dim);
		font-size: var(--fs-sm);
	}
	.file-inventory li {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-1) var(--space-2);
		padding: 2px 0;
		overflow-wrap: anywhere;
	}
	.inventory-skipped li span:last-child {
		color: var(--text-faint);
	}
	.file-inventory :global(.inventory-more) {
		padding: 0;
	}
	.examples {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: var(--space-2);
		margin: var(--space-1) 0 0;
	}
	/* Keep starter prompt labels aligned to the left. */
	.examples :global(.example) {
		text-align: left;
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
	.egs {
		color: var(--text-faint);
	}
	.personalize {
		margin-top: 22px;
		padding-top: 18px;
		border-top: 1px solid var(--border);
		color: var(--text-faint);
		font-size: var(--fs-sm);
	}
	.personalize code {
		font-family: var(--mono);
		color: var(--text-dim);
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
