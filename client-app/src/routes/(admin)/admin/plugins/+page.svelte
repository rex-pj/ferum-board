<script lang="ts">
	import { enhance } from '$app/forms';
	import { ROUTES } from '$lib/routes';
	import type { PageData, ActionData } from './$types';

	let { data, form }: { data: PageData; form: ActionData } = $props();

	let showInstallModal = $state(false);
	let installStep = $state<'upload' | 'review' | 'confirm'>('upload');
	let reviewData = $state<any>(null);
	let fileInput = $state<HTMLInputElement | null>(null);
	let selectedFile = $state<File | null>(null);
	let confirmingUninstall = $state<string | null>(null);
	let uninstallConfirmInput = $state('');
	let isSubmitting = $state(false);

	const TIER_BADGES: Record<string, string> = {
		manifest: 'bg-success',
		script: 'bg-warning text-dark',
		service: 'bg-danger'
	};

	const STATUS_BADGES: Record<string, string> = {
		active: 'bg-success',
		inactive: 'bg-secondary',
		error: 'bg-danger',
		installing: 'bg-info',
		disabled: 'bg-secondary',
		uninstalling: 'bg-warning text-dark'
	};

	function handleFileChange(e: Event) {
		const input = e.target as HTMLInputElement;
		selectedFile = input.files?.[0] ?? null;
	}

	function openInstallModal() {
		showInstallModal = true;
		installStep = 'upload';
		reviewData = null;
		selectedFile = null;
	}

	function closeInstallModal() {
		showInstallModal = false;
		installStep = 'upload';
		reviewData = null;
	}

	// When upload returns review data, advance to review step
	$effect(() => {
		if (form?.review) {
			reviewData = form.review;
			installStep = 'review';
		}
		if (form?.plugin) {
			closeInstallModal();
		}
	});
</script>

<svelte:head>
	<title>Plugins — Admin</title>
</svelte:head>

<div class="d-flex justify-content-between align-items-center mb-4">
	<h1 class="h3 mb-0">Plugins</h1>
	<button class="btn btn-primary" onclick={openInstallModal}>
		<i class="fa-solid fa-puzzle-piece me-1"></i>Install Plugin
	</button>
</div>

{#if form?.error}
	<div class="alert alert-danger">{form.error}</div>
{/if}
{#if form?.success && !form?.plugin}
	<div class="alert alert-success">{form.success}</div>
{/if}

<!-- Plugin list -->
{#if data.plugins.length === 0}
	<div class="card">
		<div class="card-body text-center py-5 text-muted">
			<i class="fa-solid fa-puzzle-piece fa-3x mb-3 opacity-25"></i>
			<p class="mb-0">No plugins installed. Click "Install Plugin" to get started.</p>
		</div>
	</div>
{:else}
	<div class="list-group">
		{#each data.plugins as plugin (plugin.slug)}
			<div class="list-group-item">
				<div class="d-flex align-items-start gap-3">
					<div class="flex-grow-1">
						<div class="d-flex align-items-center gap-2 mb-1">
							<a href={ROUTES.ADMIN.PLUGIN(plugin.slug)} class="fw-semibold text-decoration-none">
								{plugin.name}
							</a>
							<span class="badge {TIER_BADGES[plugin.tier] ?? 'bg-secondary'} small">
								{plugin.tier}
							</span>
							<span class="badge {STATUS_BADGES[plugin.status] ?? 'bg-secondary'} small">
								{plugin.status}
							</span>
							{#if plugin.circuit_open}
								<span class="badge bg-danger small">Circuit open</span>
							{/if}
						</div>
						<small class="text-muted">
							v{plugin.version}
							· Installed {new Date(plugin.installed_at).toLocaleDateString()}
							{#if plugin.activated_at}
								· Active since {new Date(plugin.activated_at).toLocaleDateString()}
							{/if}
						</small>
						{#if plugin.error_message}
							<div class="text-danger small mt-1">{plugin.error_message}</div>
						{/if}
					</div>

					<div class="d-flex gap-2 flex-shrink-0">
						<a href={ROUTES.ADMIN.PLUGIN(plugin.slug)} class="btn btn-sm btn-outline-secondary">
							Configure
						</a>

						<form method="POST" action="?/toggleStatus" use:enhance={() => {
							isSubmitting = true;
							return async ({ update }) => { isSubmitting = false; update(); };
						}}>
							<input type="hidden" name="slug" value={plugin.slug} />
							<input type="hidden" name="active" value={plugin.status === 'active' ? 'false' : 'true'} />
							<button
								type="submit"
								class="btn btn-sm {plugin.status === 'active' ? 'btn-outline-warning' : 'btn-outline-success'}"
								disabled={isSubmitting}
							>
								{plugin.status === 'active' ? 'Disable' : 'Enable'}
							</button>
						</form>

						<button
							class="btn btn-sm btn-outline-danger"
							onclick={() => { confirmingUninstall = plugin.slug; uninstallConfirmInput = ''; }}
						>
							Uninstall
						</button>
					</div>
				</div>
			</div>
		{/each}
	</div>
{/if}

<!-- Install modal -->
{#if showInstallModal}
	<div class="modal d-block" tabindex="-1" style="background: rgba(0,0,0,.5)">
		<div class="modal-dialog modal-lg">
			<div class="modal-content">
				<div class="modal-header">
					<h5 class="modal-title">
						{#if installStep === 'upload'}Upload Plugin{/if}
						{#if installStep === 'review'}Review Capabilities{/if}
					</h5>
					<button type="button" class="btn-close" onclick={closeInstallModal}></button>
				</div>

				{#if installStep === 'upload'}
					<form method="POST" action="?/uploadPlugin" enctype="multipart/form-data" use:enhance={() => {
						isSubmitting = true;
						return async ({ update }) => { isSubmitting = false; update(); };
					}}>
						<div class="modal-body">
							<p class="text-muted small">Upload a <code>.fpkg</code> plugin archive to begin installation.</p>
							<div class="mb-3">
								<label class="form-label fw-semibold" for="plugin-file">Plugin package (.fpkg)</label>
								<input
									id="plugin-file"
									type="file"
									name="file"
									accept=".fpkg"
									class="form-control"
									required
									onchange={handleFileChange}
									bind:this={fileInput}
								/>
							</div>
							{#if form?.error}
								<div class="alert alert-danger py-2 mb-0">{form.error}</div>
							{/if}
						</div>
						<div class="modal-footer">
							<button type="button" class="btn btn-secondary" onclick={closeInstallModal}>Cancel</button>
							<button type="submit" class="btn btn-primary" disabled={isSubmitting}>
								{isSubmitting ? 'Uploading…' : 'Upload & Review'}
							</button>
						</div>
					</form>

				{:else if installStep === 'review' && reviewData}
					<form method="POST" action="?/installPlugin" enctype="multipart/form-data" use:enhance={() => {
						isSubmitting = true;
						return async ({ update }) => { isSubmitting = false; update(); };
					}}>
						<div class="modal-body">
							<!-- Plugin summary -->
							<div class="d-flex align-items-center gap-2 mb-3">
								<div>
									<div class="fw-semibold fs-5">{reviewData.name}</div>
									<small class="text-muted">
										v{reviewData.version} · {reviewData.tier} plugin
										{#if reviewData.author}· by {reviewData.author}{/if}
									</small>
								</div>
								<span class="badge {TIER_BADGES[reviewData.tier] ?? 'bg-secondary'} ms-auto">
									{reviewData.tier}
								</span>
							</div>

							{#if reviewData.tier !== 'manifest'}
								<div class="alert alert-warning py-2 small">
									<i class="fa-solid fa-triangle-exclamation me-1"></i>
									<strong>Tier {reviewData.tier}</strong> —
									{reviewData.tier === 'script'
										? 'This plugin runs sandboxed JavaScript code on your server.'
										: 'This plugin runs as a separate process and can own a database schema.'}
								</div>
							{/if}

							<h6 class="mb-2">Requested capabilities:</h6>
							<pre class="rounded p-3 small mb-3" style="max-height:180px;overflow:auto;background:var(--bs-tertiary-bg);color:var(--bs-body-color)">{JSON.stringify(reviewData.capabilities_requested, null, 2)}</pre>

							<!-- Re-upload required for step 2 (browser security prevents re-using a prior file input) -->
							<div class="mb-3">
								<label class="form-label fw-semibold" for="install-file">
									Re-select the same .fpkg file to confirm
								</label>
								<input
									id="install-file"
									type="file"
									name="file"
									accept=".fpkg"
									class="form-control"
									required
								/>
								<div class="form-text">Browser security requires re-selecting the file for confirmation.</div>
							</div>

							<input type="hidden" name="granted_capabilities" value={JSON.stringify(reviewData.capabilities_requested)} />

							{#if form?.error}
								<div class="alert alert-danger py-2 mb-0">{form.error}</div>
							{/if}
						</div>
						<div class="modal-footer">
							<button type="button" class="btn btn-secondary" onclick={() => { installStep = 'upload'; reviewData = null; }}>
								Back
							</button>
							<button type="submit" class="btn btn-primary" disabled={isSubmitting}>
								{isSubmitting ? 'Installing…' : 'Grant & Install'}
							</button>
						</div>
					</form>
				{/if}
			</div>
		</div>
	</div>
{/if}

<!-- Uninstall confirm modal -->
{#if confirmingUninstall}
	{@const pluginToUninstall = data.plugins.find((p: { slug: string }) => p.slug === confirmingUninstall)}
	<div class="modal d-block" tabindex="-1" style="background: rgba(0,0,0,.5)">
		<div class="modal-dialog">
			<div class="modal-content border-danger">
				<div class="modal-header bg-danger text-white">
					<h5 class="modal-title">Confirm Uninstall</h5>
					<button type="button" class="btn-close btn-close-white" onclick={() => confirmingUninstall = null}></button>
				</div>
				<form method="POST" action="?/uninstall" use:enhance={() => {
					isSubmitting = true;
					return async ({ update }) => { isSubmitting = false; confirmingUninstall = null; update(); };
				}}>
					<div class="modal-body">
						<p>This will permanently uninstall <strong>{pluginToUninstall?.name}</strong>.</p>
						{#if pluginToUninstall?.tier === 'service'}
							<div class="alert alert-danger py-2">
								<i class="fa-solid fa-triangle-exclamation me-1"></i>
								This plugin owns a database schema. All plugin data will be <strong>permanently deleted</strong>.
							</div>
						{/if}
						<div class="mb-3">
							<label class="form-label">Type <code>{confirmingUninstall}</code> to confirm:</label>
							<input
								type="text"
								class="form-control"
								bind:value={uninstallConfirmInput}
								placeholder={confirmingUninstall ?? ''}
								autocomplete="off"
							/>
						</div>
						<input type="hidden" name="slug" value={confirmingUninstall} />
						<input type="hidden" name="confirm_slug" value={uninstallConfirmInput} />
					</div>
					<div class="modal-footer">
						<button type="button" class="btn btn-secondary" onclick={() => confirmingUninstall = null}>Cancel</button>
						<button
							type="submit"
							class="btn btn-danger"
							disabled={uninstallConfirmInput !== confirmingUninstall || isSubmitting}
						>
							Uninstall
						</button>
					</div>
				</form>
			</div>
		</div>
	</div>
{/if}
