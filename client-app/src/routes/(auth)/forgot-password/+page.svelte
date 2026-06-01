<svelte:head>
	<title>Reset Password | Ferum Board</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<script lang="ts">
	import { enhance } from '$app/forms';
	import { ROUTES } from '$lib/routes';

	let { form }: { form: any } = $props();
	let loading = $state(false);
</script>

{#if form?.success}
	<div class="text-center py-3">
		<div class="fr-auth-icon-circle mx-auto mb-3">
			<i class="fa-solid fa-paper-plane"></i>
		</div>
		<h2 class="fr-auth-heading mb-1">Check your inbox</h2>
		<p class="fr-auth-sub mb-4">If that email address is registered, we've sent a reset link. It expires in 1 hour.</p>
		<a href={ROUTES.LOGIN} class="btn btn-outline-secondary w-100">Back to sign in</a>
	</div>
{:else}
	<div class="text-center mb-4">
		<div class="fr-auth-icon-circle mx-auto mb-3">
			<i class="fa-solid fa-lock-open"></i>
		</div>
		<h1 class="fr-auth-heading">Forgot your password?</h1>
		<p class="fr-auth-sub">Enter your email and we'll send a reset link.</p>
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
		<div class="mb-4">
			<label for="email" class="form-label fw-medium">Email address</label>
			<input
				type="email"
				id="email"
				name="email"
				class="form-control"
				required
				autocomplete="email"
			/>
		</div>

		<button type="submit" class="btn btn-primary w-100" disabled={loading}>
			{#if loading}
				<span class="spinner-border spinner-border-sm me-2" role="status" aria-hidden="true"></span>
			{/if}
			Send reset link
		</button>
	</form>

	<div class="fr-auth-divider">or</div>

	<p class="text-center mb-0 small">
		<a href={ROUTES.LOGIN} class="fw-medium">Back to sign in</a>
	</p>
{/if}
