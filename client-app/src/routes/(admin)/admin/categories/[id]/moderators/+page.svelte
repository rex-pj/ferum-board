<svelte:head>
	<title>Category Moderators | Admin | {data.siteName ?? 'Ferum Board'}</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<script lang="ts">
	import { enhance } from '$app/forms';
	import { ROUTES } from '$lib/routes';
	import { toast } from '$lib/stores/toast';

	let { data, form }: { data: any; form: any } = $props();
	let assigning = $state(false);
	let revokingId = $state<string | null>(null);

	// ── Typeahead state ──────────────────────────────────────────────────────────
	type UserHit = { id: string; username: string; display_name: string | null; is_banned: boolean; primary_role_slug: string | null };

	let searchText = $state('');
	let searchResults = $state<UserHit[]>([]);
	let selectedUser = $state<UserHit | null>(null);
	let searching = $state(false);
	let showDropdown = $state(false);
	let debounce: ReturnType<typeof setTimeout>;

	function handleInput() {
		selectedUser = null;
		clearTimeout(debounce);
		if (searchText.trim().length < 2) {
			searchResults = [];
			showDropdown = false;
			return;
		}
		debounce = setTimeout(doSearch, 300);
	}

	async function doSearch() {
		searching = true;
		try {
			const res = await fetch(`/api/admin/users/search?q=${encodeURIComponent(searchText)}&per_page=8`);
			if (res.ok) {
				const json = await res.json();
				searchResults = json.data ?? [];
				showDropdown = searchResults.length > 0;
			}
		} finally {
			searching = false;
		}
	}

	// onmousedown fires before onblur so the selection registers before the dropdown closes
	function pickUser(user: UserHit) {
		selectedUser = user;
		searchText = user.display_name ? `${user.display_name} (@${user.username})` : `@${user.username}`;
		showDropdown = false;
	}

	function closeDropdown() {
		setTimeout(() => { showDropdown = false; }, 150);
	}

	function resetForm() {
		selectedUser = null;
		searchText = '';
		searchResults = [];
		showDropdown = false;
	}
</script>

<div class="d-flex align-items-center gap-2 mb-4">
	<a href={ROUTES.ADMIN.CATEGORY(data.categoryId)} class="btn btn-outline-secondary btn-sm" aria-label="Back to category">
		<i class="fa-solid fa-arrow-left"></i>
	</a>
	<h1 class="h4 mb-0">Moderators for {data.categoryName}</h1>
</div>

{#if form?.error}
	<div class="alert alert-danger" role="alert">{form.error}</div>
{/if}

<div class="card mb-4">
	<div class="card-header">Assign moderator</div>
	<div class="card-body">
		<form
			method="POST"
			action="?/assign"
			use:enhance={() => {
				assigning = true;
				return async ({ result, update }) => {
					assigning = false;
					if (result.type === 'success') {
						toast.success('Moderator assigned.');
						resetForm();
					}
					await update();
				};
			}}
		>
			<input type="hidden" name="user_id" value={selectedUser?.id ?? ''} />

			<div class="row g-2 align-items-end">
				<div class="col position-relative">
					<label class="form-label" for="user_search">Search user</label>
					<div class="input-group">
						<span class="input-group-text"><i class="fa-solid fa-magnifying-glass"></i></span>
						<input
							type="text"
							id="user_search"
							class="form-control"
							placeholder="Type username or display name…"
							bind:value={searchText}
							oninput={handleInput}
							onblur={closeDropdown}
							autocomplete="off"
						/>
						{#if searching}
							<span class="input-group-text">
								<span class="spinner-border spinner-border-sm text-secondary"></span>
							</span>
						{/if}
					</div>

					{#if showDropdown}
						<ul class="list-group position-absolute w-100 shadow-sm border" style="z-index:1050;top:calc(100% + 2px);left:0;max-height:280px;overflow-y:auto">
							{#each searchResults as user}
								<li class="list-group-item list-group-item-action p-0">
									<button
										type="button"
										class="btn w-100 text-start px-3 py-2 d-flex align-items-center gap-2"
										onmousedown={() => pickUser(user)}
									>
										<div class="flex-grow-1 min-width-0">
											<div class="fw-semibold text-truncate">
												{user.display_name ?? user.username}
												{#if user.is_banned}
													<span class="badge bg-danger ms-1">Banned</span>
												{/if}
											</div>
											<div class="text-muted small">@{user.username}{user.primary_role_slug ? ` · ${user.primary_role_slug}` : ''}</div>
										</div>
									</button>
								</li>
							{/each}
						</ul>
					{/if}
				</div>

				<div class="col-auto">
					<button type="submit" class="btn btn-primary" disabled={assigning || !selectedUser}>
						{#if assigning}<span class="spinner-border spinner-border-sm me-1"></span>{/if}
						Assign
					</button>
				</div>
			</div>

			{#if selectedUser}
				<div class="mt-2 d-flex align-items-center gap-2">
					<i class="fa-solid fa-circle-check text-success"></i>
					<span class="small">
						Selected: <strong>{selectedUser.display_name ?? selectedUser.username}</strong>
						<span class="text-muted">(@{selectedUser.username})</span>
					</span>
					<button type="button" class="btn btn-link btn-sm p-0 text-muted ms-1" onclick={resetForm} aria-label="Clear selection">
						<i class="fa-solid fa-xmark"></i>
					</button>
				</div>
			{/if}
		</form>
	</div>
</div>

<div class="table-responsive">
	<table class="table table-hover align-middle">
		<thead>
			<tr>
				<th>User</th>
				<th>Assigned at</th>
				<th>Assigned by</th>
				<th></th>
			</tr>
		</thead>
		<tbody>
			{#each data.moderators as mod}
				<tr>
					<td>
						<div class="fw-semibold">{mod.display_name ?? mod.username}</div>
						<code class="small text-muted">@{mod.username}</code>
					</td>
					<td class="text-muted small">{new Date(mod.created_at).toLocaleString()}</td>
					<td class="text-muted small">
						{#if mod.granted_by}
							<code class="small">{mod.granted_by.slice(0, 8)}…</code>
						{:else}
							<span class="text-muted">—</span>
						{/if}
					</td>
					<td>
						<form
							method="POST"
							action="?/revoke"
							use:enhance={() => {
								revokingId = mod.user_id;
								return async ({ result, update }) => {
									revokingId = null;
									if (result.type === 'success') toast.success('Moderator revoked.');
									await update();
								};
							}}
						>
							<input type="hidden" name="user_id" value={mod.user_id} />
							<button
								type="submit"
								class="btn btn-outline-danger btn-sm"
								disabled={revokingId === mod.user_id}
								onclick={(e) => { if (!confirm('Revoke moderator?')) e.preventDefault(); }}
							>
								{#if revokingId === mod.user_id}
									<span class="spinner-border spinner-border-sm"></span>
								{:else}
									<i class="fa-solid fa-xmark"></i>
								{/if}
								Revoke
							</button>
						</form>
					</td>
				</tr>
			{/each}
			{#if data.moderators.length === 0}
				<tr>
					<td colspan="4" class="text-center text-muted py-4">No moderators assigned.</td>
				</tr>
			{/if}
		</tbody>
	</table>
</div>
