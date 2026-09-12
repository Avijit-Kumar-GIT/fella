<script lang="ts">
	import { dispatch, openFolder, resumeLastFolder } from '$lib/commands';
	import { session } from '$lib/session.svelte';
	import { isTauri, openExternal } from '$lib/ipc';
	import { fadeQuick } from '$lib/motion';
	import Icon from './Icon.svelte';
	import Message from './Message.svelte';

	// The empty screen adapts to what's already set up, so a non-technical user
	// always sees the one next step rather than a bare "not connected".
	let providerId = $derived(session.settings?.provider ?? 'ollama');
	let provider = $derived(session.providers.find((p) => p.id === providerId));
	let providerName = $derived(provider?.display ?? providerId);
	let getKeyUrl = $derived(provider?.get_key_url ?? '');

	let up = $derived(session.health?.reachable === true);
	let rejected = $derived(session.health?.rejected === true);
	let currentModel = $derived(session.settings?.model ?? '');
	let healthModels = $derived(session.health?.models ?? []);

	// A local Ollama, probed regardless of which provider is selected.
	let ollamaUp = $derived(session.ollamaLocal?.reachable === true);
	let ollamaModelCount = $derived(session.ollamaLocal?.models?.length ?? 0);

	// On Ollama, reachable, but nothing but embedding models are pulled.
	let ollamaNoChat = $derived(
		providerId === 'ollama' &&
			up &&
			healthModels.length > 0 &&
			!healthModels.some((m) => !/embed/i.test(m))
	);
	// Connected to a hosted service but no model chosen yet (gateways ship with
	// no default; we don't auto-pick from hundreds).
	let needModelPick = $derived(up && providerId !== 'ollama' && !currentModel);

	// Show the connect panel whenever the user can't actually ask a question.
	let showSetup = $derived(rejected || !up || ollamaNoChat || needModelPick);
	let showExamples = $derived(up && !!currentModel && !ollamaNoChat && !needModelPick);

	let services = $derived(
		session.providers.filter((p) => p.auth !== 'none' && p.id !== 'custom')
	);

	/** Open the provider's key page (if any) and start the paste flow. */
	function connectService(id: string, keyUrl: string) {
		if (keyUrl) void openExternal(keyUrl);
		void dispatch(`/login ${id}`);
	}

	let hasFolder = $derived(!!session.catalog.workspace);
	let folderName = $derived(
		session.catalog.workspace?.replace(/[/\\]+$/, '').replace(/^.*[/\\]/, '') ?? ''
	);
	let lastFolderName = $derived(
		session.lastFolder?.replace(/[/\\]+$/, '').replace(/^.*[/\\]/, '') ?? ''
	);
	let fileCount = $derived(session.catalog.sources.length);
	let skipped = $derived(session.catalog.skipped ?? []);

	const EXAMPLES = [
		'How did my spending change this year?',
		'What stands out in my workout log?',
		"Summarise what's in these files"
	];

	let scroller: HTMLDivElement;
	let expanded = $state<Record<string, boolean>>({});

	function toggle(id: string) {
		expanded = { ...expanded, [id]: !expanded[id] };
	}

	let stick = true;
	function onScroll() {
		if (!scroller) return;
		stick = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 40;
	}
	// Switching tabs shows a different transcript jump it to the latest and
	// drop any stale expanded-evidence state from the previous tab. Entering
	// focus mode collapses evidence too, so it's just the answers.
	$effect(() => {
		session.active;
		session.focus;
		stick = true;
		expanded = {};
	});
	$effect(() => {
		session.messages.length;
		session.messages.at(-1)?.text;
		if (stick && scroller) {
			queueMicrotask(() => scroller.scrollTo({ top: scroller.scrollHeight }));
		}
	});

	export function collapseAll() {
		expanded = {};
	}
</script>

<!-- role="log" for structure, but not a live region the answer streams in
     token by token, which would make a screen reader read every fragment. The
     visually-hidden status line in +page.svelte does the announcing instead. -->
<div
	class="transcript"
	class:focus={session.focus}
	bind:this={scroller}
	onscroll={onScroll}
	role="log"
	aria-label="conversation"
	aria-live="off"
>
	{#if session.messages.length === 0}
		<div class="onboard" class:center={!hasFolder && !showSetup}>
			<div class="wordmark" aria-label="Fella">Fella</div>
			{#if !hasFolder}
				<span class="glyph" aria-hidden="true"><Icon name="asterisk" size={44} /></span>
			{/if}
			<h1 class="hero">Ask about your own files</h1>

			{#if !hasFolder}
				<div class="cta">
					{#if session.lastFolder}
						<button
							class="pill primary"
							title="Reopen your last folder (Enter)"
							onclick={() => void resumeLastFolder()}
						>
							<Icon name="folder" size={14} /> Reopen {lastFolderName}
						</button>
						<button class="pill" onclick={() => void openFolder()}>Choose another</button>
					{:else}
						<button class="pill primary" onclick={() => void openFolder()}>
							<Icon name="folder" size={14} /> Choose a folder
						</button>
					{/if}
				</div>
				{#if isTauri()}<p class="drophint">or drag a folder onto this window</p>{/if}
				<p class="trust">
					Spreadsheets, PDFs, notes anything in one folder. Answered on your
					computer, from your files, never changed.
				</p>
			{:else if fileCount === 0}
				<p class="lead"><strong>{folderName}</strong> is open, but nothing in it is readable yet.</p>
				<p>
					Fella works with spreadsheets, CSVs, Excel, PDFs and plain text.{#if skipped.length}
						<code>/files</code> shows what was skipped and why.{/if}
				</p>
				<div class="cta">
					<button class="pill primary" onclick={() => void openFolder()}>
						<Icon name="folder" size={14} /> Choose a different folder
					</button>
				</div>
			{:else}
				<p class="lead">
					<strong>{folderName}</strong> · {fileCount} file{fileCount === 1 ? '' : 's'} ready.
				</p>
				{#if showExamples}
					<p class="egs">Try one:</p>
					<div class="examples">
						{#each EXAMPLES as q (q)}
							<button class="pill example" onclick={() => void dispatch(q)}>{q}</button>
						{/each}
					</div>
				{/if}
			{/if}

			{#if showSetup}
				<div class="setup">
					{#if rejected}
						<!-- Hosted key was refused (HTTP 401/403). -->
						<p>Your <strong>{providerName}</strong> key was refused.</p>
						<div class="svc">
							<button class="pill" onclick={() => void dispatch(`/login ${providerId}`)}>Enter a new key</button>
							{#if getKeyUrl}
								<button class="pill ghost" onclick={() => void openExternal(getKeyUrl)}>
									Get a new key <Icon name="arrow-up-right" size={13} />
								</button>
							{/if}
						</div>
					{:else if needModelPick}
						<!-- Connected to a hosted service, no model chosen. -->
						<p>Connected to <strong>{providerName}</strong>. Choose a model to finish.</p>
						{#if healthModels.length && healthModels.length <= 12}
							<div class="svc">
								{#each healthModels as m (m)}
									<button class="pill" onclick={() => void dispatch(`/model ${m}`)}>{m}</button>
								{/each}
							</div>
						{:else}
							<p class="alt">Run <code>/model</code> to see what's available and pick one.</p>
						{/if}
					{:else if ollamaNoChat}
						<!-- Ollama up but only embedding models pulled. -->
						<p>
							<strong>Ollama</strong> is running, but no chat model is downloaded yet.
							In a terminal: <code>ollama pull llama3.1</code>
						</p>
					{:else if providerId === 'ollama'}
						<!-- Default path: Ollama selected but not reachable. -->
						<p>
							Fella isn't connected to a model. It sends your questions to
							<strong>Ollama</strong>, a free app that runs on your own computer.
						</p>
						<ol class="steps">
							<li>
								Install it from
								<button class="link" onclick={() => void openExternal('https://ollama.com/download')}>ollama.com</button>
								(skip if you already have it).
							</li>
							<li>
								Open Ollama so it's running. It sits in your menu bar or tray; on
								Linux you may need <code>ollama serve</code> in a terminal.
							</li>
							<li>Download a model: <code>ollama pull llama3.1</code></li>
						</ol>
						<p class="alt">Fella keeps checking, so there's no need to restart it.</p>
						{#if services.length}
							<p class="alt">Or connect an online service instead (you paste in a key):</p>
							<div class="svc">
								{#each services as p (p.id)}
									<button class="pill" onclick={() => connectService(p.id, p.get_key_url)}>{p.display}</button>
								{/each}
							</div>
						{/if}
					{:else if ollamaUp}
						<!-- On a hosted provider that's down, but a local Ollama is running. -->
						<p>
							<strong>Ollama</strong> is running on this computer
							{#if ollamaModelCount}({ollamaModelCount} model{ollamaModelCount === 1 ? '' : 's'}){/if}.
						</p>
						<div class="svc">
							<button class="pill" onclick={() => void dispatch('/model provider ollama')}>Use Ollama</button>
						</div>
						<p class="alt">
							Or fix <strong>{providerName}</strong>:
							<button class="link" onclick={() => void dispatch(`/login ${providerId}`)}>enter a key</button>.
						</p>
					{:else}
						<!-- Hosted provider unreachable, no local Ollama. -->
						<p>Can't reach <strong>{providerName}</strong>. Check your internet connection.</p>
						{#if services.length}
							<p class="alt">Or connect a different service:</p>
							<div class="svc">
								{#each services as p (p.id)}
									<button class="pill" onclick={() => connectService(p.id, p.get_key_url)}>{p.display}</button>
								{/each}
							</div>
						{/if}
					{/if}
				</div>
			{/if}

			{#if hasFolder && showExamples}
				<p class="personalize">
					Make it yours: <code>/packs browse</code> for themes and skills, or drop a
					<code>fella.md</code> in this folder to tell Fella how your files are organised.
				</p>
			{/if}
		</div>
	{:else}
		{#if showSetup}
			<!-- The full onboarding panel only shows on the empty screen. If the
			     model stops working mid-session, keep a compact version in view so
			     the user isn't left with just a red dot and bare error lines. -->
			<div class="setup compact stream">
				{#if rejected}
					<p>
						Your <strong>{providerName}</strong> key was refused.
						<button class="link" onclick={() => void dispatch(`/login ${providerId}`)}>
							Enter a new key
						</button>
					</p>
				{:else if needModelPick}
					<p>
						Connected to <strong>{providerName}</strong>, but no model is chosen.
						<button class="link" onclick={() => void dispatch('/model')}>Pick a model</button>
					</p>
				{:else if ollamaNoChat}
					<p>
						<strong>Ollama</strong> is running, but no chat model is downloaded.
						Run <code>ollama pull llama3.1</code>.
					</p>
				{:else if providerId === 'ollama'}
					<p>
						Fella can't reach <strong>Ollama</strong>. Open the Ollama app, or run
						<code>ollama serve</code>. Fella keeps checking.
					</p>
				{:else if ollamaUp}
					<p>
						Can't reach <strong>{providerName}</strong>.
						<button class="link" onclick={() => void dispatch('/model provider ollama')}>
							Use local Ollama
						</button>
						, or
						<button class="link" onclick={() => void dispatch(`/login ${providerId}`)}>
							enter a key
						</button>.
					</p>
				{:else}
					<p>Can't reach <strong>{providerName}</strong>. Check your internet connection.</p>
				{/if}
			</div>
		{/if}
		{#key session.active}
			<div class="stream" in:fadeQuick>
				<svelte:boundary>
					{#each session.messages as m (m.id)}
						<Message message={m} expanded={!!expanded[m.id]} ontoggle={() => toggle(m.id)} />
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
		max-width: 52ch;
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
	.wordmark {
		font-size: var(--fs-sm);
		font-weight: 500;
		letter-spacing: 0.02em;
		color: var(--text-faint);
		margin: 0 0 var(--space-6);
	}
	.glyph {
		display: inline-flex;
		color: var(--brand);
		margin: 0 0 var(--space-3);
	}
	.hero {
		font-size: 26px;
		font-weight: 650;
		letter-spacing: -0.01em;
		text-wrap: balance;
		color: var(--text);
		margin: 0 0 var(--space-3);
	}
	.onboard p {
		margin: 0 0 var(--space-3);
	}
	.onboard .lead {
		color: var(--text);
		font-size: var(--fs-lg);
	}
	.onboard .lead strong {
		font-weight: 600;
	}
	.examples {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: var(--space-2);
		margin: var(--space-1) 0 0;
	}
	/* .pill provides the look; the example buttons only need left text. */
	.example {
		text-align: left;
	}
	.cta {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-3);
		margin: var(--space-6) 0 var(--space-2);
	}
	.onboard.center .cta {
		justify-content: center;
	}
	.cta .pill {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
	}
	.drophint {
		color: var(--text-faint);
		font-size: var(--fs-sm);
	}
	.trust {
		color: var(--text-faint);
		font-size: var(--fs-sm);
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
	.setup .steps {
		margin: 0 0 10px;
		padding-left: 1.4em;
	}
	.setup .steps li {
		margin-bottom: 6px;
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
	/* An inline text button that reads as a link, for use mid-sentence. */
	.setup button.link {
		border: none;
		padding: 0;
		border-radius: 0;
		color: var(--link);
		background: transparent;
	}
	.setup button.link:hover {
		background: transparent;
		text-decoration: underline;
	}
	.boundary-err {
		font-family: var(--mono);
		font-size: var(--fs-sm);
		color: var(--warn);
		white-space: pre-wrap;
		word-break: break-word;
	}
</style>
