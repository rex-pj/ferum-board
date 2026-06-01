<svelte:head>
	<title>Account Settings | Ferum Board</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<script lang="ts">
	import { enhance } from '$app/forms';
	import { invalidate } from '$app/navigation';
	import { theme as themeStore, type Theme } from '$lib/stores/theme';
	import Avatar from '$lib/components/atoms/Avatar.svelte';
	import Badge from '$lib/components/atoms/Badge.svelte';
	import { ROUTES } from '$lib/routes';
	import { toast } from '$lib/stores/toast';

	let { data, form }: { data: any; form: any } = $props();
	const u = $derived(data.profile);
	const prefs = $derived(data.prefs);

	type Tab = 'profile' | 'security' | 'preferences';
	let activeTab = $state<Tab>('profile');

	let savingProfile = $state(false);
	let savingPassword = $state(false);
	let savingPrefs = $state(false);

	let avatarUrl = $state<string | null>(data.profile?.avatar_url ?? null);
	let avatarUploading = $state(false);

	let bioLength = $state((data.profile?.bio ?? '').length);
	let displayNameLength = $state((data.profile?.display_name ?? '').length);

	let newPassword = $state('');
	let confirmPassword = $state('');
	let passwordMismatch = $derived(confirmPassword.length > 0 && newPassword !== confirmPassword);
	let confirmValid = $derived(confirmPassword.length >= 8 && !passwordMismatch);

	let selectedTheme = $state<Theme>(prefs?.theme ?? 'auto');
	let selectedFontSize = $state<string>(prefs?.font_size ?? 'medium');
	let selectedLayout = $state<string>(prefs?.layout ?? 'comfortable');

	// Navigate to the right tab when a server-side error comes back
	$effect(() => {
		if (form?.profileError) activeTab = 'profile';
		if (form?.passwordError) activeTab = 'security';
		if (form?.prefsError) activeTab = 'preferences';
	});

	async function uploadAvatar(e: Event) {
		const input = e.target as HTMLInputElement;
		const file = input.files?.[0];
		if (!file) return;
		avatarUploading = true;
		const fd = new FormData();
		fd.append('file', file);
		try {
			const res = await fetch('/api/users/me/avatar', { method: 'POST', body: fd });
			if (!res.ok) {
				const body = await res.json().catch(() => ({}));
				toast.error(body?.error?.message ?? 'Upload failed.');
			} else {
				const body = await res.json();
				avatarUrl = body.data?.avatar_url ?? avatarUrl;
				toast.success('Avatar updated.');
			}
		} catch {
			toast.error('Network error during upload.');
		} finally {
			avatarUploading = false;
			input.value = '';
		}
	}

	async function removeAvatar() {
		avatarUploading = true;
		try {
			const res = await fetch('/api/users/me/avatar', { method: 'DELETE' });
			if (!res.ok) {
				toast.error('Failed to remove avatar.');
			} else {
				avatarUrl = null;
				toast.success('Avatar removed.');
			}
		} catch {
			toast.error('Network error.');
		} finally {
			avatarUploading = false;
		}
	}
</script>

<div class="fr-content-layout">
<!-- Main column -->
<div class="fr-feed-col">
	<h1 class="h4 mb-3">Account Settings</h1>

	<!-- Tab nav -->
	<ul class="nav nav-tabs mb-4">
		<li class="nav-item">
			<button
				type="button"
				class="nav-link {activeTab === 'profile' ? 'active' : ''}"
				onclick={() => (activeTab = 'profile')}
			>
				<i class="fa-solid fa-user fa-sm me-1"></i>Profile
			</button>
		</li>
		<li class="nav-item">
			<button
				type="button"
				class="nav-link {activeTab === 'security' ? 'active' : ''}"
				onclick={() => (activeTab = 'security')}
			>
				<i class="fa-solid fa-lock fa-sm me-1"></i>Security
			</button>
		</li>
		<li class="nav-item">
			<button
				type="button"
				class="nav-link {activeTab === 'preferences' ? 'active' : ''}"
				onclick={() => (activeTab = 'preferences')}
			>
				<i class="fa-solid fa-sliders fa-sm me-1"></i>Preferences
			</button>
		</li>
	</ul>

	<!-- Profile tab -->
	{#if activeTab === 'profile'}
		<div class="card">
			<div class="card-body">
				{#if form?.profileError}
					<div class="alert alert-danger py-2 mb-3">{form.profileError}</div>
				{/if}

				<!-- Avatar row -->
				<div class="d-flex align-items-center gap-3 mb-4 pb-4 border-bottom">
					<div style="flex-shrink:0;">
						<Avatar src={avatarUrl} username={u?.username ?? ''} size={72} />
					</div>
					<div>
						<div class="fw-semibold mb-1" style="color: var(--bs-body-color);">
							{u?.display_name ?? u?.username}
						</div>
						<div class="small text-muted mb-2">@{u?.username}</div>
						<div class="d-flex gap-2 flex-wrap">
							<label class="btn btn-outline-secondary btn-sm" style="cursor:pointer; min-height:44px; display:inline-flex; align-items:center;">
								{#if avatarUploading}
									<span class="spinner-border spinner-border-sm me-1"></span>Uploading…
								{:else}
									<i class="fa-solid fa-camera me-1"></i>{avatarUrl ? 'Change photo' : 'Upload photo'}
								{/if}
								<input
									type="file"
									accept="image/jpeg,image/png,image/webp,image/gif"
									class="d-none"
									disabled={avatarUploading}
									onchange={uploadAvatar}
								/>
							</label>
							{#if avatarUrl}
								<button
									type="button"
									class="btn btn-outline-danger btn-sm"
									style="min-height:44px;"
									disabled={avatarUploading}
									onclick={removeAvatar}
								>
									<i class="fa-solid fa-trash me-1"></i>Remove
								</button>
							{/if}
						</div>
					</div>
				</div>

				<!-- Profile form -->
				<form
					method="POST"
					action="?/updateProfile"
					use:enhance={() => {
						savingProfile = true;
						return async ({ result, update }) => {
							savingProfile = false;
							if (result.type === 'success') toast.success('Profile saved.');
							await update({ reset: false });
							if (result.type === 'success') await invalidate('app:user');
						};
					}}
				>
					<div class="mb-3">
						<label class="form-label d-flex justify-content-between" for="display_name">
							<span>Display name</span>
							<span class="text-muted small fw-normal">{displayNameLength}/80</span>
						</label>
						<input
							type="text"
							id="display_name"
							name="display_name"
							class="form-control"
							maxlength="80"
							value={u?.display_name ?? ''}
							placeholder="Leave blank to use username"
							oninput={(e) => (displayNameLength = (e.target as HTMLInputElement).value.length)}
						/>
					</div>
					<div class="mb-3">
						<label class="form-label d-flex justify-content-between" for="bio">
							<span>Bio</span>
							<span class="text-muted small fw-normal">{bioLength}/500</span>
						</label>
						<textarea
							id="bio"
							name="bio"
							class="form-control"
							rows="3"
							maxlength="500"
							oninput={(e) => (bioLength = (e.target as HTMLTextAreaElement).value.length)}
						>{u?.bio ?? ''}</textarea>
					</div>
					<div class="mb-4">
						<label class="form-label" for="website">Website</label>
						<div class="input-group">
							<span class="input-group-text"><i class="fa-solid fa-link fa-sm"></i></span>
							<input
								type="url"
								id="website"
								name="website"
								class="form-control"
								maxlength="200"
								value={u?.website ?? ''}
								placeholder="https://example.com"
							/>
						</div>
					</div>
					<button type="submit" class="btn btn-primary" style="min-height:44px;" disabled={savingProfile}>
						{#if savingProfile}<span class="spinner-border spinner-border-sm me-2"></span>{/if}
						Save Profile
					</button>
				</form>
			</div>
		</div>
	{/if}

	<!-- Security tab -->
	{#if activeTab === 'security'}
		<div class="card mb-3">
			<div class="card-header fw-semibold">Change Password</div>
			<div class="card-body">
				{#if form?.passwordError}
					<div class="alert alert-danger py-2 mb-3">{form.passwordError}</div>
				{/if}
				<form
					method="POST"
					action="?/changePassword"
					use:enhance={() => {
						savingPassword = true;
						return async ({ result, update }) => {
							savingPassword = false;
							if (result.type === 'success') {
								newPassword = '';
								confirmPassword = '';
								toast.success('Password changed successfully.');
							}
							await update();
						};
					}}
				>
					<div class="mb-3">
						<label class="form-label" for="current_password">Current password</label>
						<input
							type="password"
							id="current_password"
							name="current_password"
							class="form-control"
							required
							autocomplete="current-password"
						/>
					</div>
					<div class="mb-3">
						<label class="form-label" for="new_password">New password</label>
						<input
							type="password"
							id="new_password"
							name="new_password"
							class="form-control"
							required
							minlength="8"
							autocomplete="new-password"
							bind:value={newPassword}
						/>
						<div class="form-text">Minimum 8 characters.</div>
					</div>
					<div class="mb-4">
						<label class="form-label" for="confirm_password">Confirm new password</label>
						<input
							type="password"
							id="confirm_password"
							class="form-control {passwordMismatch ? 'is-invalid' : confirmValid ? 'is-valid' : ''}"
							required
							minlength="8"
							autocomplete="new-password"
							bind:value={confirmPassword}
						/>
						{#if passwordMismatch}
							<div class="invalid-feedback">Passwords do not match.</div>
						{:else if confirmValid}
							<div class="valid-feedback">Looks good!</div>
						{/if}
					</div>
					<button
						type="submit"
						class="btn btn-primary"
						style="min-height:44px;"
						disabled={savingPassword || passwordMismatch || newPassword.length < 8 || confirmPassword.length < 8}
					>
						{#if savingPassword}<span class="spinner-border spinner-border-sm me-2"></span>{/if}
						Change Password
					</button>
				</form>
			</div>
		</div>

		<div class="card border-0 bg-body-tertiary">
			<div class="card-body py-3">
				<p class="small fw-semibold text-muted text-uppercase mb-2" style="letter-spacing:.05em;">
					Tips for a strong password
				</p>
				<ul class="small mb-0 ps-3" style="color: var(--bs-secondary-color); line-height: 1.8;">
					<li>Use at least 8 characters</li>
					<li>Mix letters, numbers &amp; symbols</li>
					<li>Don't reuse passwords from other sites</li>
				</ul>
			</div>
		</div>
	{/if}

	<!-- Preferences tab -->
	{#if activeTab === 'preferences'}
		<div class="card">
			<div class="card-body">
				{#if form?.prefsError}
					<div class="alert alert-danger py-2 mb-3">{form.prefsError}</div>
				{/if}
				<form
					method="POST"
					action="?/updatePreferences"
					use:enhance={({ formData }) => {
						savingPrefs = true;
						const newTheme = formData.get('theme') as Theme;
						return async ({ result, update }) => {
							savingPrefs = false;
							if (result.type === 'success') {
								if (newTheme) themeStore.set(newTheme);
								toast.success('Preferences saved.');
							}
							await update({ reset: false });
							if (result.type === 'success') await invalidate('app:preferences');
						};
					}}
				>
					<!-- Hidden inputs carry state to the server action -->
					<input type="hidden" name="theme" value={selectedTheme} />
					<input type="hidden" name="font_size" value={selectedFontSize} />
					<input type="hidden" name="layout" value={selectedLayout} />

					<!-- Theme -->
					<div class="mb-4">
						<label class="form-label fw-semibold d-block mb-2">Theme</label>
						<div class="d-flex gap-2 flex-wrap">
							{#each [
								{ value: 'auto', icon: 'fa-circle-half-stroke', label: 'Auto' },
								{ value: 'light', icon: 'fa-sun', label: 'Light' },
								{ value: 'dark', icon: 'fa-moon', label: 'Dark' }
							] as opt (opt.value)}
								<button
									type="button"
									class="btn {selectedTheme === opt.value ? 'btn-primary' : 'btn-outline-secondary'} d-flex flex-column align-items-center justify-content-center gap-1"
									style="min-width:80px; min-height:70px;"
									onclick={() => (selectedTheme = opt.value as Theme)}
								>
									<i class="fa-solid {opt.icon}"></i>
									<span class="small">{opt.label}</span>
								</button>
							{/each}
						</div>
					</div>

					<!-- Font size -->
					<div class="mb-4">
						<label class="form-label fw-semibold d-block mb-2">Font Size</label>
						<div class="btn-group" role="group" aria-label="Font size">
							{#each [
								{ value: 'small', label: 'Small' },
								{ value: 'medium', label: 'Medium' },
								{ value: 'large', label: 'Large' }
							] as opt (opt.value)}
								<button
									type="button"
									class="btn {selectedFontSize === opt.value ? 'btn-primary' : 'btn-outline-secondary'}"
									style="min-height:44px; min-width:90px;"
									onclick={() => (selectedFontSize = opt.value)}
								>
									{opt.label}
								</button>
							{/each}
						</div>
					</div>

					<!-- Layout -->
					<div class="mb-4">
						<label class="form-label fw-semibold d-block mb-2">Post Layout</label>
						<div class="d-flex gap-2 flex-wrap">
							{#each [
								{ value: 'comfortable', icon: 'fa-align-justify', label: 'Comfortable', desc: 'More spacing' },
								{ value: 'compact', icon: 'fa-bars', label: 'Compact', desc: 'Denser list' }
							] as opt (opt.value)}
								<button
									type="button"
									class="btn {selectedLayout === opt.value ? 'btn-primary' : 'btn-outline-secondary'} d-flex flex-column align-items-center justify-content-center gap-1"
									style="min-width:110px; min-height:80px;"
									onclick={() => (selectedLayout = opt.value)}
								>
									<i class="fa-solid {opt.icon}"></i>
									<span class="small fw-semibold">{opt.label}</span>
									<span class="x-small" style="font-size:0.7rem; opacity:0.75;">{opt.desc}</span>
								</button>
							{/each}
						</div>
					</div>

					<button type="submit" class="btn btn-primary" style="min-height:44px;" disabled={savingPrefs}>
						{#if savingPrefs}<span class="spinner-border spinner-border-sm me-2"></span>{/if}
						Save Preferences
					</button>
				</form>
			</div>
		</div>
	{/if}
</div>

<!-- Right panel -->
<aside class="fr-right-panel">
	{#if u}
		<div class="fr-panel">
			<div class="fr-panel-header">Your Account</div>
			<div class="px-3 py-3 d-flex align-items-center gap-3">
				<div style="flex-shrink:0;">
					<Avatar src={avatarUrl} username={u.username} size={40} />
				</div>
				<div style="min-width:0;">
					<div class="fw-semibold text-truncate" style="color: var(--bs-body-color);">
						{u.display_name ?? u.username}
					</div>
					<div class="small text-muted text-truncate">@{u.username}</div>
					<div class="d-flex gap-1 mt-1 flex-wrap">
						<Badge role={u.role} />
						<Badge trust={u.trust_level} />
					</div>
				</div>
			</div>
			<div class="fr-panel-stat border-top" style="border-top: 1px solid var(--bs-border-color);">
				<span style="color: var(--bs-secondary-color);">Email</span>
				<span
					class="small text-truncate ms-2"
					style="color: var(--bs-body-color); max-width:140px;"
					title={u.email}
				>{u.email}</span>
			</div>
		</div>
	{/if}

	<div class="fr-panel">
		<div class="fr-panel-header">Quick Links</div>
		<div class="fr-panel-body">
			<a href={ROUTES.NOTIFICATIONS} class="fr-panel-row">
				<i class="fa-solid fa-bell fa-sm" style="width:1rem; opacity:0.5; flex-shrink:0;"></i>
				<span style="color: var(--bs-body-color); font-size: 0.8125rem;">Notifications</span>
			</a>
			<a href={ROUTES.BOOKMARKS} class="fr-panel-row">
				<i class="fa-solid fa-bookmark fa-sm" style="width:1rem; opacity:0.5; flex-shrink:0;"></i>
				<span style="color: var(--bs-body-color); font-size: 0.8125rem;">Bookmarks</span>
			</a>
			{#if u}
				<a href={ROUTES.USER_PROFILE(u.username)} class="fr-panel-row">
					<i class="fa-regular fa-user fa-sm" style="width:1rem; opacity:0.5; flex-shrink:0;"></i>
					<span style="color: var(--bs-body-color); font-size: 0.8125rem;">Public Profile</span>
				</a>
			{/if}
			<a href={ROUTES.HOME} class="fr-panel-row">
				<i class="fa-solid fa-house fa-sm" style="width:1rem; opacity:0.5; flex-shrink:0;"></i>
				<span style="color: var(--bs-body-color); font-size: 0.8125rem;">Home Feed</span>
			</a>
		</div>
	</div>
</aside>
</div>
