<svelte:head>
	<title>Settings | Admin | Ferum Board</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<script lang="ts">
	import { enhance } from '$app/forms';
	import { invalidate } from '$app/navigation';
	import { toast } from '$lib/stores/toast';

	let { data, form }: { data: any; form: any } = $props();
	let saving = $state(false);

	const cfg = $derived(data.config ?? {});
	const webhooks: any[] = $derived(data.webhooks ?? []);

	type Tab = 'general' | 'webhooks';
	let activeTab = $state<Tab>('general');

	// Live preview state for branding fields
	let logoUrlInput = $state(data.config?.logo_url ?? '');

	// Keep the Branding URL input in sync with logo upload/remove
	$effect(() => { logoUrlInput = currentLogoUrl ?? ''; });
	let logoPreviewError = $state(false);
	let colorInput = $state(data.config?.primary_color ?? '#0d6efd');

	// Logo upload state
	let logoFile = $state<File | null>(null);
	let logoObjectUrl = $state<string | null>(null);
	let uploadingLogo = $state(false);
	let removingLogo = $state(false);
	// Tracks the live logo URL; updated optimistically after upload/remove
	let currentLogoUrl = $state<string | null>(cfg.logo_url || null);

	function onLogoChange(e: Event) {
		const input = e.target as HTMLInputElement;
		const file = input.files?.[0] ?? null;
		if (logoObjectUrl) URL.revokeObjectURL(logoObjectUrl);
		logoFile = file;
		logoObjectUrl = file ? URL.createObjectURL(file) : null;
	}

	// Favicon state
	let faviconFile = $state<File | null>(null);
	let faviconObjectUrl = $state<string | null>(null);
	let uploadingFavicon = $state(false);
	let removingFavicon = $state(false);
	// Tracks the live favicon URL; updated optimistically after upload/remove
	let currentFaviconUrl = $state<string | null>(cfg.favicon_url || null);

	function onFaviconChange(e: Event) {
		const input = e.target as HTMLInputElement;
		const file = input.files?.[0] ?? null;
		if (faviconObjectUrl) URL.revokeObjectURL(faviconObjectUrl);
		faviconFile = file;
		faviconObjectUrl = file ? URL.createObjectURL(file) : null;
	}

	// Navigate to the right tab when a server error comes back
	$effect(() => {
		if (form?.error) activeTab = 'general';
	});

	// Webhook create form state
	let newWebhookUrl = $state('');
	let newWebhookEvents = $state('post.created');
	let newWebhookSecret = $state('');
	let creatingWebhook = $state(false);

	const WEBHOOK_MAX_FAILURES = 5;

	const AVAILABLE_EVENTS = [
		'post.created',
		'post.deleted',
		'thread.locked',
		'thread.moved',
		'reaction.added',
		'thread.best_answer_marked',
		'user.banned',
		'user.warned'
	];
</script>

<div class="d-flex align-items-center justify-content-between mb-3">
	<h1 class="h4 mb-0">Site Settings</h1>
</div>

<!-- Tab nav -->
<ul class="nav nav-tabs mb-4">
	<li class="nav-item">
		<button
			type="button"
			class="nav-link {activeTab === 'general' ? 'active' : ''}"
			onclick={() => (activeTab = 'general')}
		>
			<i class="fa-solid fa-gear fa-sm me-1"></i>General
		</button>
	</li>
	<li class="nav-item">
		<button
			type="button"
			class="nav-link {activeTab === 'webhooks' ? 'active' : ''}"
			onclick={() => (activeTab = 'webhooks')}
		>
			<i class="fa-solid fa-webhook fa-sm me-1"></i>Webhooks
			{#if webhooks.length > 0}
				<span class="badge bg-secondary ms-1 badge-xs">{webhooks.length}</span>
			{/if}
		</button>
	</li>
</ul>

<!-- ─── General tab ─────────────────────────────────────────────────────────── -->
{#if activeTab === 'general'}
	{#if form?.error}
		<div class="alert alert-danger mb-4" role="alert">{form.error}</div>
	{/if}

	<!-- Branding — one card: logo/favicon uploads + name/slogan/tagline/color.
	     HTML forbids nested forms, so upload forms are siblings inside the card;
	     the save-form inputs use form="saveForm" to associate with the form below. -->
	<div class="card mb-3">
		<div class="card-header fw-semibold">Branding</div>
		<div class="card-body">
			<div class="row g-4">

				<!-- Logo upload -->
				<div class="col-md-6">
					<p class="form-label fw-medium mb-2">Logo</p>
					<div class="d-flex align-items-start gap-3 flex-wrap">
						<div class="d-flex flex-column align-items-center gap-1">
							<div class="border rounded p-2 d-flex align-items-center justify-content-center logo-preview">
								{#if logoObjectUrl || currentLogoUrl}
									<img src={logoObjectUrl || currentLogoUrl} alt="Current logo" class="logo-img" />
								{:else}
									<i class="fa-solid fa-image fa-lg text-muted"></i>
								{/if}
							</div>
							<span class="small text-muted">Current</span>
						</div>
						<div class="d-flex flex-column flex-grow-1 gap-sm">
							<form method="POST" action="?/uploadLogo" enctype="multipart/form-data" class="m-0"
								use:enhance={() => {
									uploadingLogo = true;
									return async ({ result, update }) => {
										uploadingLogo = false;
										if (result.type === 'success') {
											currentLogoUrl = (result.data?.logoUrl as string | null) ?? currentLogoUrl;
											if (logoObjectUrl) { URL.revokeObjectURL(logoObjectUrl); logoObjectUrl = null; }
											logoFile = null;
											toast.success('Logo updated.');
										}
										await update({ reset: false });
									};
								}}
							>
								<div class="input-group input-md">
									<input type="file" name="file" class="form-control"
										accept=".jpg,.jpeg,.png,.webp,.gif,image/jpeg,image/png,image/webp,image/gif"
										onchange={onLogoChange} />
									<button type="submit" class="btn btn-outline-primary" disabled={uploadingLogo || !logoFile}>
										{#if uploadingLogo}<span class="spinner-border spinner-border-sm me-1"></span>{/if}
										Upload
									</button>
								</div>
								<div class="form-text m-0">Accepted: JPEG, PNG, WebP, GIF · Max 2 MB</div>
							</form>
							{#if currentLogoUrl}
								<form method="POST" action="?/removeLogo" class="m-0"
									use:enhance={() => {
										removingLogo = true;
										return async ({ result, update }) => {
											removingLogo = false;
											if (result.type === 'success') { currentLogoUrl = null; toast.success('Logo removed.'); }
											await update({ reset: false });
										};
									}}
								>
									<button type="submit" class="btn btn-sm btn-outline-danger" disabled={removingLogo}
										onclick={(e) => { if (!confirm('Remove the logo?')) e.preventDefault(); }}>
										{#if removingLogo}<span class="spinner-border spinner-border-sm me-1"></span>{/if}
										<i class="fa-solid fa-trash fa-sm me-1"></i>Remove logo
									</button>
								</form>
							{/if}
						</div>
					</div>
					<!-- Logo URL — lives here so it's clearly tied to the logo upload above -->
					<div class="mt-3">
						<label class="form-label small text-muted mb-1" for="logo_url">Logo URL</label>
						<input type="text" id="logo_url" name="logo_url" form="saveForm"
							class="form-control form-control-sm font-monospace" maxlength="500"
							bind:value={logoUrlInput}
							oninput={() => (logoPreviewError = false)}
							placeholder="https://example.com/logo.png or /files/…" />
						<div class="form-text">Set automatically on upload, or enter an external URL.</div>
						{#if logoUrlInput && logoPreviewError}
							<div class="mt-1 small text-danger">
								<i class="fa-solid fa-triangle-exclamation me-1"></i>Could not load image — check the URL.
							</div>
						{/if}
					</div>
				</div>

				<!-- Favicon upload -->
				<div class="col-md-6">
					<p class="form-label fw-medium mb-2">Favicon</p>
					<div class="d-flex align-items-start gap-3 flex-wrap">
						<div class="d-flex flex-column align-items-center gap-1">
							<div class="border rounded p-2 d-flex align-items-center justify-content-center favicon-preview">
								{#if faviconObjectUrl || currentFaviconUrl}
									<img src={faviconObjectUrl || currentFaviconUrl} alt="Current favicon" class="favicon-img" />
								{:else}
									<i class="fa-solid fa-image fa-lg text-muted"></i>
								{/if}
							</div>
							<span class="small text-muted">Current</span>
						</div>
						<div class="d-flex flex-column flex-grow-1 gap-sm">
							<form method="POST" action="?/uploadFavicon" enctype="multipart/form-data" class="m-0"
								use:enhance={() => {
									uploadingFavicon = true;
									return async ({ result, update }) => {
										uploadingFavicon = false;
										if (result.type === 'success') {
											currentFaviconUrl = (result.data?.faviconUrl as string | null) ?? currentFaviconUrl;
											if (faviconObjectUrl) { URL.revokeObjectURL(faviconObjectUrl); faviconObjectUrl = null; }
											faviconFile = null;
											toast.success('Favicon updated.');
											await invalidate('app:config');
										}
										await update({ reset: false });
									};
								}}
							>
								<div class="input-group input-md">
									<input type="file" name="file" class="form-control"
										accept=".ico,.svg,.png,.gif,.jpg,.jpeg,image/x-icon,image/svg+xml,image/png,image/gif,image/jpeg"
										onchange={onFaviconChange} />
									<button type="submit" class="btn btn-outline-primary" disabled={uploadingFavicon || !faviconFile}>
										{#if uploadingFavicon}<span class="spinner-border spinner-border-sm me-1"></span>{/if}
										Upload
									</button>
								</div>
								<div class="form-text m-0">Accepted: ICO, SVG, PNG, GIF, JPEG · Max 512 KB</div>
							</form>
							{#if currentFaviconUrl}
								<form method="POST" action="?/removeFavicon" class="m-0"
									use:enhance={() => {
										removingFavicon = true;
										return async ({ result, update }) => {
											removingFavicon = false;
											if (result.type === 'success') {
												currentFaviconUrl = null;
												toast.success('Favicon removed.');
												await invalidate('app:config');
											}
											await update({ reset: false });
										};
									}}
								>
									<button type="submit" class="btn btn-sm btn-outline-danger" disabled={removingFavicon}
										onclick={(e) => { if (!confirm('Remove the custom favicon?')) e.preventDefault(); }}>
										{#if removingFavicon}<span class="spinner-border spinner-border-sm me-1"></span>{/if}
										<i class="fa-solid fa-trash fa-sm me-1"></i>Remove favicon
									</button>
								</form>
							{/if}
						</div>
					</div>
				</div>

				<div class="col-12"><hr class="my-0" /></div>

				<!-- Site Name + Slogan (save form) -->
				<div class="col-md-6">
					<label class="form-label" for="site_name">Site Name</label>
					<input type="text" id="site_name" name="site_name" form="saveForm"
						class="form-control" maxlength="100"
						value={cfg.site_name ?? ''} placeholder="Ferum Board" />
				</div>
				<div class="col-md-6">
					<label class="form-label" for="site_slogan">Slogan</label>
					<input type="text" id="site_slogan" name="site_slogan" form="saveForm"
						class="form-control" maxlength="80"
						value={cfg.site_slogan ?? ''} placeholder="Your space, your rules" />
					<div class="form-text">Short text shown beside the logo in the header.</div>
				</div>
				<div class="col-md-6">
					<label class="form-label" for="site_tagline">Tagline</label>
					<input type="text" id="site_tagline" name="site_tagline" form="saveForm"
						class="form-control" maxlength="200"
						value={cfg.site_tagline ?? ''} placeholder="A place for discussion" />
					<div class="form-text">Shown in page meta descriptions (SEO).</div>
				</div>

				<!-- Primary Color -->
				<div class="col-md-6">
					<label class="form-label" for="primary_color">Primary Color</label>
					<div class="d-flex align-items-center gap-3">
						<input type="color" id="primary_color" name="primary_color" form="saveForm"
							class="form-control form-control-color color-picker"
							bind:value={colorInput} />
						<div class="d-flex align-items-center gap-2">
							<button type="button" class="btn btn-sm px-3"
								style="background:{colorInput}; border-color:{colorInput}; color:#fff; min-height:36px;">
								Sample button
							</button>
							<code class="small text-muted">{colorInput}</code>
						</div>
					</div>
					<div class="form-text">Applied as the primary accent color across the entire site.</div>
				</div>

			</div>
		</div>
	</div>

	<form
		id="saveForm"
		method="POST"
		action="?/save"
		use:enhance={() => {
			saving = true;
			return async ({ result, update }) => {
				saving = false;
				if (result.type === 'success') toast.success('Settings saved.');
				await update({ reset: false });
			};
		}}
	>
		<!-- Registration -->
		<div class="card mb-3">
			<div class="card-header fw-semibold">Registration</div>
			<div class="card-body">
				<div class="form-check form-switch">
					<input
						class="form-check-input"
						type="checkbox"
						role="switch"
						id="registration_open"
						name="registration_open"
						value="true"
						checked={cfg.registration_open !== 'false'}
					/>
					<label class="form-check-label" for="registration_open">
						Allow new user registration
					</label>
				</div>
				<div class="form-text mt-1">
					When disabled, the registration page returns 403 and invite links stop working.
				</div>
			</div>
		</div>

		<!-- Content Moderation -->
		<div class="card mb-4">
			<div class="card-header fw-semibold">Content Moderation</div>
			<div class="card-body">
				<label class="form-label" for="keyword_blacklist">Keyword Blacklist</label>
				<textarea
					id="keyword_blacklist"
					name="keyword_blacklist"
					class="form-control font-monospace"
					rows="4"
					placeholder="One keyword per line"
				>{cfg.keyword_blacklist ?? ''}</textarea>
				<div class="form-text">Posts containing these keywords will be flagged for moderator review.</div>
			</div>
		</div>

		<button type="submit" class="btn btn-primary" style="min-height:44px;" disabled={saving}>
			{#if saving}<span class="spinner-border spinner-border-sm me-2"></span>{/if}
			Save Settings
		</button>
	</form>
{/if}

<!-- ─── Webhooks tab ─────────────────────────────────────────────────────────── -->
{#if activeTab === 'webhooks'}
	<p class="text-muted small mb-4">
		Webhooks fire an HTTP POST to your URL when forum events occur.
		Optionally set a secret to verify the <code>X-Ferum-Signature</code> header (HMAC-SHA256).
		A webhook is auto-disabled after {WEBHOOK_MAX_FAILURES} consecutive delivery failures.
	</p>

	{#if webhooks.length > 0}
		<div class="list-group mb-4">
			{#each webhooks as hook}
				<div class="list-group-item py-3">
					<div class="d-flex justify-content-between align-items-start gap-2">
						<div class="flex-grow-1 overflow-hidden">
							<div class="d-flex align-items-center gap-2 mb-1 flex-wrap">
								<span class="badge {hook.is_active ? 'bg-success' : 'bg-secondary'}">
									{hook.is_active ? 'Active' : 'Disabled'}
								</span>
								{#if !hook.is_active && hook.failure_count >= WEBHOOK_MAX_FAILURES}
									<span class="badge bg-danger" title="Auto-disabled after repeated failures">
										<i class="fa-solid fa-triangle-exclamation me-1"></i>Circuit open
									</span>
								{/if}
								<code class="small text-truncate">{hook.url}</code>
							</div>
							<div class="small text-muted">
								Events: {hook.events.join(', ')}
								{#if hook.has_secret}
									· <i class="fa-solid fa-lock me-1"></i>Signed
								{/if}
								{#if hook.last_triggered_at}
									· Last fired: {new Date(hook.last_triggered_at).toLocaleString()}
								{/if}
								{#if hook.failure_count > 0}
									· <span class="text-danger">
										<i class="fa-solid fa-circle-exclamation me-1"></i>{hook.failure_count} failure{hook.failure_count === 1 ? '' : 's'}
									</span>
								{/if}
							</div>
						</div>
						<div class="d-flex gap-2 flex-shrink-0">
							<form
								method="POST"
								action="?/toggleWebhook"
								use:enhance={({ formData }) => {
									const active = formData.get('is_active') === 'true';
									return async ({ result, update }) => {
										if (result.type === 'success')
											toast.success(active ? 'Webhook enabled.' : 'Webhook disabled.');
										await update();
									};
								}}
							>
								<input type="hidden" name="id" value={hook.id} />
								<input type="hidden" name="is_active" value={hook.is_active ? 'false' : 'true'} />
								<button
									type="submit"
									class="btn btn-sm btn-outline-secondary webhook-btn"
									title={hook.is_active ? 'Disable webhook' : 'Re-enable webhook'}
								>
									{hook.is_active ? 'Disable' : 'Re-enable'}
								</button>
							</form>
							<form
								method="POST"
								action="?/deleteWebhook"
								use:enhance={() => {
									return async ({ result, update }) => {
										if (result.type === 'success') toast.success('Webhook deleted.');
										await update();
									};
								}}
							>
								<input type="hidden" name="id" value={hook.id} />
								<button
									type="submit"
									class="btn btn-sm btn-outline-danger webhook-btn"
									onclick={(e) => {
										if (!confirm('Delete this webhook?')) e.preventDefault();
									}}
								>
									<i class="fa-solid fa-trash"></i>
								</button>
							</form>
						</div>
					</div>
				</div>
			{/each}
		</div>
	{:else}
		<div class="text-muted small mb-4 py-3 text-center border rounded border-dashed">
			No webhooks configured yet.
		</div>
	{/if}

	<!-- Add webhook form -->
	<div class="card">
		<div class="card-header fw-semibold">Add Webhook</div>
		<div class="card-body">
			<form
				method="POST"
				action="?/createWebhook"
				use:enhance={() => {
					creatingWebhook = true;
					return async ({ result, update }) => {
						creatingWebhook = false;
						newWebhookUrl = '';
						newWebhookSecret = '';
						if (result.type === 'success') toast.success('Webhook created.');
						await update();
					};
				}}
			>
				<div class="row g-3">
					<div class="col-md-6">
						<label class="form-label" for="wh_url">
							URL <span class="text-danger">*</span>
						</label>
						<input
							type="url"
							id="wh_url"
							name="url"
							class="form-control"
							placeholder="https://example.com/webhook"
							bind:value={newWebhookUrl}
							required
						/>
					</div>
					<div class="col-md-6">
						<label class="form-label" for="wh_secret">Secret <span class="text-muted fw-normal">(optional)</span></label>
						<input
							type="text"
							id="wh_secret"
							name="secret"
							class="form-control font-monospace"
							placeholder="Leave blank for unsigned"
							bind:value={newWebhookSecret}
						/>
					</div>
					<div class="col-12">
						<label class="form-label">Events <span class="text-danger">*</span></label>
						<div class="d-flex flex-wrap gap-2">
							{#each AVAILABLE_EVENTS as evt}
								<div class="form-check">
									<input
										class="form-check-input"
										type="checkbox"
										name="events"
										value={evt}
										id="evt_{evt}"
										checked={evt === 'post.created'}
									/>
									<label class="form-check-label small" for="evt_{evt}">{evt}</label>
								</div>
							{/each}
						</div>
					</div>
				</div>
				<button
					type="submit"
					class="btn btn-primary mt-3"
					disabled={creatingWebhook || !newWebhookUrl}
				>
					{#if creatingWebhook}<span class="spinner-border spinner-border-sm me-2"></span>{/if}
					Add Webhook
				</button>
			</form>
		</div>
	</div>
{/if}

<style>
	.badge-xs { font-size: 0.65rem; }

	.logo-preview {
		width: 80px;
		height: 56px;
		background: var(--bs-tertiary-bg);
	}

	.logo-img {
		max-width: 64px;
		max-height: 40px;
		object-fit: contain;
	}

	.favicon-preview {
		width: 48px;
		height: 48px;
		background: var(--bs-tertiary-bg);
	}

	.favicon-img {
		max-width: 32px;
		max-height: 32px;
		object-fit: contain;
	}

	.gap-sm { gap: 6px; }
	.input-md { max-width: 380px; }

	.logo-url-preview {
		background: var(--bs-tertiary-bg);
		width: fit-content;
	}

	.logo-url-img {
		max-height: 40px;
		max-width: 200px;
		object-fit: contain;
	}

	.color-picker { width: 60px; height: 44px; }
	.webhook-btn { min-height: var(--fr-tap-target); }
	.border-dashed { border-style: dashed !important; }

	/* min-height from the tap-target rule stretches file inputs beyond their
	   natural height, leaving dead space below the native button. */
	input[type="file"].form-control { min-height: unset !important; }
</style>
