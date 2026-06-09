<script lang="ts">
	import { enhance } from '$app/forms';
	import { ROUTES } from '$lib/routes';
	import type { PageData, ActionData } from './$types';

	let { data, form }: { data: PageData; form: ActionData } = $props();

	let activeTab = $state<'config' | 'logs'>('config');
	let isSubmitting = $state(false);

	const plugin = $derived(data.plugin);

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

	const LOG_LEVEL_CLASSES: Record<string, string> = {
		trace: 'text-muted',
		info: 'text-info',
		warn: 'text-warning',
		error: 'text-danger'
	};

	// Build config form fields from JSON Schema
	interface SchemaProperty {
		type: string;
		title?: string;
		description?: string;
		default?: unknown;
		enum?: string[];
		minimum?: number;
		maximum?: number;
	}

	const configFields = $derived(() => {
		const schema = plugin?.config_schema;
		if (!schema?.properties) return [];
		return Object.entries(schema.properties as Record<string, SchemaProperty>).map(([key, prop]) => ({
			key,
			type: prop.type ?? 'string',
			title: prop.title ?? key,
			description: prop.description,
			defaultValue: prop.default,
			enumValues: prop.enum,
			min: prop.minimum,
			max: prop.maximum,
			required: (schema.required as string[] | undefined)?.includes(key) ?? false,
			currentValue: plugin?.config?.[key] ?? prop.default ?? ''
		}));
	});
</script>

<svelte:head>
	<title>{plugin?.name ?? 'Plugin'} — Admin</title>
</svelte:head>

<nav aria-label="breadcrumb" class="mb-3">
	<ol class="breadcrumb">
		<li class="breadcrumb-item"><a href={ROUTES.ADMIN.PLUGINS}>Plugins</a></li>
		<li class="breadcrumb-item active">{plugin?.name}</li>
	</ol>
</nav>

<!-- Header -->
<div class="d-flex align-items-start justify-content-between mb-4">
	<div>
		<div class="d-flex align-items-center gap-2 mb-1">
			<h1 class="h3 mb-0">{plugin?.name}</h1>
			<span class="badge {TIER_BADGES[plugin?.tier] ?? 'bg-secondary'}">
				{plugin?.tier}
			</span>
			<span class="badge {STATUS_BADGES[plugin?.status] ?? 'bg-secondary'}">
				{plugin?.status}
			</span>
			{#if plugin?.circuit_open}
				<span class="badge bg-danger">Circuit open</span>
			{/if}
		</div>
		<small class="text-muted">v{plugin?.version} · {plugin?.slug}</small>
	</div>

	<div class="d-flex gap-2">
		{#if plugin?.status === 'active'}
			<form method="POST" action="?/deactivate" use:enhance={() => {
				isSubmitting = true;
				return async ({ update }) => { isSubmitting = false; update(); };
			}}>
				<button class="btn btn-outline-warning" disabled={isSubmitting}>Disable</button>
			</form>
		{:else if plugin?.status === 'inactive'}
			<form method="POST" action="?/activate" use:enhance={() => {
				isSubmitting = true;
				return async ({ update }) => { isSubmitting = false; update(); };
			}}>
				<button class="btn btn-outline-success" disabled={isSubmitting}>Enable</button>
			</form>
		{/if}
	</div>
</div>

{#if form?.error}
	<div class="alert alert-danger">{form.error}</div>
{/if}
{#if form?.success}
	<div class="alert alert-success">{form.success}</div>
{/if}

{#if plugin?.error_message}
	<div class="alert alert-danger">
		<i class="fa-solid fa-circle-exclamation me-1"></i>
		<strong>Plugin error:</strong> {plugin.error_message}
	</div>
{/if}

<!-- Tabs -->
<ul class="nav nav-tabs mb-4">
	<li class="nav-item">
		<button
			class="nav-link {activeTab === 'config' ? 'active' : ''}"
			onclick={() => activeTab = 'config'}
		>
			Configuration
		</button>
	</li>
	<li class="nav-item">
		<button
			class="nav-link {activeTab === 'logs' ? 'active' : ''}"
			onclick={() => activeTab = 'logs'}
		>
			Logs
			{#if data.logs?.some((l: any) => l.level === 'error')}
				<span class="badge bg-danger ms-1">{data.logs.filter((l: any) => l.level === 'error').length}</span>
			{/if}
		</button>
	</li>
</ul>

{#if activeTab === 'config'}
	{#if configFields().length === 0}
		<div class="card">
			<div class="card-body text-muted text-center py-4">
				This plugin has no configuration options.
			</div>
		</div>
	{:else}
		<form method="POST" action="?/configure" use:enhance={() => {
			isSubmitting = true;
			return async ({ update }) => { isSubmitting = false; update(); };
		}}>
			<div class="card">
				<div class="card-body">
					{#each configFields() as field (field.key)}
						<div class="mb-3">
							<label class="form-label fw-semibold" for="cfg-{field.key}">
								{field.title}
								{#if field.required}<span class="text-danger">*</span>{/if}
							</label>

							{#if field.enumValues}
								<select id="cfg-{field.key}" name={field.key} class="form-select" required={field.required}>
									{#each field.enumValues as opt}
										<option value={opt} selected={field.currentValue === opt}>{opt}</option>
									{/each}
								</select>
							{:else if field.type === 'boolean'}
								<div class="form-check form-switch">
									<input
										id="cfg-{field.key}"
										type="checkbox"
										name={field.key}
										class="form-check-input"
										role="switch"
										value="true"
										checked={field.currentValue === true}
									/>
								</div>
							{:else if field.type === 'integer' || field.type === 'number'}
								<input
									id="cfg-{field.key}"
									type="number"
									name={field.key}
									class="form-control"
									value={field.currentValue}
									min={field.min}
									max={field.max}
									required={field.required}
								/>
							{:else}
								<input
									id="cfg-{field.key}"
									type="text"
									name={field.key}
									class="form-control"
									value={field.currentValue}
									required={field.required}
								/>
							{/if}

							{#if field.description}
								<div class="form-text">{field.description}</div>
							{/if}
						</div>
					{/each}
				</div>
				<div class="card-footer">
					<button type="submit" class="btn btn-primary" disabled={isSubmitting}>
						{isSubmitting ? 'Saving…' : 'Save Configuration'}
					</button>
				</div>
			</div>
		</form>
	{/if}
{:else if activeTab === 'logs'}
	<div class="card">
		<div class="card-body p-0">
			{#if data.logs?.length === 0}
				<p class="text-muted text-center py-4 mb-0">No logs yet.</p>
			{:else}
				<div class="table-responsive">
					<table class="table table-sm table-hover mb-0 font-monospace">
						<thead>
							<tr>
								<th style="width: 160px">Time</th>
								<th style="width: 60px">Level</th>
								<th style="width: 140px">Hook</th>
								<th style="width: 70px">Duration</th>
								<th>Message</th>
							</tr>
						</thead>
						<tbody>
							{#each data.logs as log (log.id)}
								<tr>
									<td class="text-muted small">
										{new Date(log.created_at).toLocaleTimeString()}
									</td>
									<td>
										<span class="{LOG_LEVEL_CLASSES[log.level] ?? ''} small fw-semibold">
											{log.level}
										</span>
									</td>
									<td class="text-muted small">{log.hook_name ?? '—'}</td>
									<td class="text-muted small">
										{log.duration_ms != null ? `${log.duration_ms}ms` : '—'}
									</td>
									<td class="small">{log.message}</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			{/if}
		</div>
	</div>
{/if}

<!-- Plugin metadata footer -->
<div class="mt-4 text-muted small">
	<div class="d-flex gap-4 flex-wrap">
		<span>ID: <code>{plugin?.slug}</code></span>
		<span>Tier: {plugin?.tier}</span>
		{#if plugin?.restart_count > 0}
			<span class="text-warning">Restarts: {plugin?.restart_count}</span>
		{/if}
		{#if plugin?.granted_capabilities && Object.keys(plugin.granted_capabilities).length > 0}
			<span>Capabilities: {Object.keys(plugin.granted_capabilities).join(', ')}</span>
		{/if}
	</div>
</div>
