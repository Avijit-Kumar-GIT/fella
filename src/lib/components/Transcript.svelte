<script lang="ts">
	import { dispatch, openFolder } from '$lib/commands';
	import { session } from '$lib/session.svelte';
	import { isTauri, openExternal } from '$lib/ipc';
	import { fadeQuick } from '$lib/motion';
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
	let fileCount = $derived(session.catalog.sources.length);
	let skipped = $derived(session.catalog.skipped ?? []);

	const EXAMPLES = [
		'How did my spending change this year?',
		'What stands out in my workout log?',
		"Summarise what's in these files"
	];

	// figlet "ANSI Shadow". Shown only on the empty screen.
	const WORDMARK = [
		'███████╗███████╗██╗     ██╗      █████╗ ',
		'██╔════╝██╔════╝██║     ██║     ██╔══██╗',
		'█████╗  █████╗  ██║     ██║     ███████║',
		'██╔══╝  ██╔══╝  ██║     ██║     ██╔══██║',
		'██║     ███████╗███████╗███████╗██║  ██║',
		'╚═╝     ╚══════╝╚══════╝╚══════╝╚═╝  ╚═╝'
	].join('\n');

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
	// drop any stale expanded-evidence state from the previous tab.
	$effect(() => {
		session.active;
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
	bind:this={scroller}
	onscroll={onScroll}
	role="log"
	aria-label="conversation"
	aria-live="off"
>
	{#if session.messages.length === 0}
		<div class="onboard">
			<pre class="wordmark" role="img" aria-label="fella">{WORDMARK}</pre>
			<h1 class="hero">Ask about your own files</h1>

			{#if !hasFolder}
				<p class="lead">
					Spending, health, workouts, notes anything you keep in a folder. Fella
					works entirely on your computer, reads your files, and never changes them.
				</p>
				<div class="cta">
					<button class="pill primary" onclick={() => void openFolder()}>Choose a folder</button>
					<span class="or">{isTauri() ? 'or drag one onto this window' : ''}</span>
				</div>
				<p class="egs">
					Then ask things like <em>“how did my spending change this year?”</em> or
					<em>“what stands out in my workout log?”</em>
				</p>
			{:else if fileCount === 0}
				<p class="lead"><strong>{folderName}</strong> is open, but Fella can't read anything in it yet.</p>
				<p>It works with spreadsheets, CSVs, Excel files, PDFs, and plain text.</p>
				{#if skipped.length}
					<p class="alt">
						{skipped.length} file{skipped.length === 1 ? '' : 's'} found but not used:
					</p>
					<ul class="skipped">
						{#each skipped.slice(0, 8) as f (f.name)}
							<li>{f.name} <span class="reason">{f.reason}</span></li>
						{/each}
						{#if skipped.length > 8}<li>and {skipped.length - 8} more</li>{/if}
					</ul>
				{/if}
				<div class="cta">
					<button class="pill primary" onclick={() => void openFolder()}>Choose a different folder</button>
				</div>
			{:else}
				<p class="lead">
					<strong>{folderName}</strong> is open. {fileCount} file{fileCount === 1 ? '' : 's'} Fella
					can read.
				</p>
				{#if skipped.length}
					<p class="alt">
						{skipped.length} other file{skipped.length === 1 ? '' : 's'} couldn't be used
						(see <code>/files</code>).
					</p>
				{/if}
				<p>
					Ask a question in plain language. Every answer shows the exact files and steps behind
					it.
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
									Get a new key ↗
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
		padding: var(--space-5) var(--pad) var(--space-6);
		min-height: 0;
	}
	/* Cap the reading column so long lines don't sprawl the "app not terminal"
	   cue. Centred in the scroller. */
	.stream {
		max-width: 76ch;
		margin-inline: auto;
	}
	.onboard {
		max-width: 60ch;
		margin: var(--space-6) auto 0;
		color: var(--text-dim);
	}
	.wordmark {
		font-family: var(--mono);
		font-size: var(--fs-xs);
		line-height: 1.15;
		color: var(--text-faint);
		margin: 0 0 var(--space-4);
		padding: 0;
		border: 0;
		background: transparent;
		overflow-x: auto;
		white-space: pre;
	}
	.hero {
		font-size: var(--fs-xl);
		font-weight: 600;
		letter-spacing: -0.02em;
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
		margin: var(--space-5) 0;
	}
	.or {
		color: var(--text-faint);
		font-size: var(--fs-sm);
	}
	.egs {
		color: var(--text-faint);
	}
	.egs em {
		font-style: italic;
		color: var(--text-dim);
	}
	.skipped {
		margin: 4px 0 0;
		padding-left: 1.1em;
		color: var(--text-dim);
		font-size: var(--fs-sm);
	}
	.skipped .reason {
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
	/* The setup / health panel is a grouped card, not a hairline-ruled section. */
	.setup {
		margin-top: var(--space-5);
		padding: var(--space-4);
		border: 1px solid var(--border);
		border-radius: var(--radius);
		background: var(--bg-raised);
		box-shadow: var(--shadow-sm);
	}
	.setup p {
		margin: 0 0 var(--space-3);
	}
	/* Mid-session health banner: not the full first-run panel, just enough to
	   point at the fix. Sits above the transcript, not below the wordmark. */
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
