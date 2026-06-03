<svelte:head>
	<title>Set New Password | Ferum Board</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<script lang="ts">
	import { enhance } from '$app/forms';
	import { page } from '$app/stores';
	import { ROUTES } from '$lib/routes';

	let { form }: { form: any } = $props();
	let loading = $state(false);
	let showPassword = $state(false);

	const token = $derived($page.url.searchParams.get('token') ?? '');
</script>

{#if !token}
	<div class="text-center py-3">
		<div class="fr-auth-icon-circle mx-auto mb-3 icon-warning">
			<i class="fa-solid fa-triangle-exclamation"></i>
		</div>
		<h2 class="fr-auth-heading mb-1">Link invalid or expired</h2>
		<p class="fr-auth-sub mb-4">This reset link is missing or has already been used. Request a new one below.</p>
		<a href={ROUTES.FORGOT_PASSWORD} class="btn btn-primary w-100">Request a new link</a>
	</div>
{:else if form?.success}
	<div class="text-center py-3">
		<div class="fr-auth-icon-circle mx-auto mb-3 icon-success">
			<i class="fa-solid fa-circle-check"></i>
		</div>
		<h2 class="fr-auth-heading mb-1">Password updated</h2>
		<p class="fr-auth-sub mb-4">Your new password is set. You can now sign in with it.</p>
		<a href={ROUTES.LOGIN} class="btn btn-primary w-100">Sign in</a>
	</div>
{:else}
	<div class="text-center mb-4">
		<div class="fr-auth-icon-circle mx-auto mb-3">
			<i class="fa-solid fa-key"></i>
		</div>
		<h1 class="fr-auth-heading">Set a new password</h1>
		<p class="fr-auth-sub">Choose a strong password you haven't used before.</p>
	</div>

	{#if form?.error}
		<div class="alert alert-danger d-flex align-items-center gap-2 py-2" role="alert">
			<i class="fa-solid fa-circle-exclamation flex-shrink-0"></i>
			<span>{form.error}</span>
		</div>
	{/if}

	<form
		method="POST"
		use:enhance={() => {
			loading = true;
			return async ({ update }) => {
				loading = false;
				await update();
			};
		}}
	>
		<input type="hidden" name="token" value={token} />

		<div class="mb-4">
			<label for="new_password" class="form-label fw-medium">New password</label>
			<div class="fr-password-wrap">
				<input
					type={showPassword ? 'text' : 'password'}
					id="new_password"
					name="new_password"
					class="form-control"
					required
					minlength="8"
					autocomplete="new-password"
				/>
				<button
					type="button"
					class="fr-password-toggle"
					onclick={() => (showPassword = !showPassword)}
					aria-label={showPassword ? 'Hide password' : 'Show password'}
				>
					<i class={showPassword ? 'fa-solid fa-eye-slash' : 'fa-solid fa-eye'}></i>
				</button>
			</div>
			<div class="form-text">At least 8 characters.</div>
		</div>

		<button type="submit" class="btn btn-primary w-100" disabled={loading}>
			{#if loading}
				<span class="spinner-border spinner-border-sm me-2" role="status" aria-hidden="true"></span>
			{/if}
			Update password
		</button>
	</form>
{/if}

<style>
	.icon-warning {
		background: color-mix(in srgb, #f59e0b 10%, var(--bs-body-bg));
		border-color: color-mix(in srgb, #f59e0b 25%, var(--bs-border-color));
		color: #f59e0b;
	}
	.icon-success {
		background: color-mix(in srgb, #10b981 10%, var(--bs-body-bg));
		border-color: color-mix(in srgb, #10b981 25%, var(--bs-border-color));
		color: #10b981;
	}
</style>