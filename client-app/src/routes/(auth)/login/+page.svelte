<svelte:head>
	<title>Login | {data.siteName ?? 'Ferum Board'}</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<script lang="ts">
	import { enhance } from '$app/forms';
	import { ROUTES } from '$lib/routes';

	let { form, data }: { form: any; data: any } = $props();
	let loading = $state(false);
	let showPassword = $state(false);
</script>

<div class="mb-4">
	<h1 class="fr-auth-heading">Welcome back</h1>
	<p class="fr-auth-sub">Sign in to your account to continue.</p>
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
	<div class="mb-3">
		<label for="email" class="form-label fw-medium">Email address</label>
		<input
			type="email"
			id="email"
			name="email"
			class="form-control"
			required
			autocomplete="email"
			value={form?.email ?? ''}
		/>
	</div>

	<div class="mb-4">
		<div class="d-flex justify-content-between align-items-center mb-1">
			<label for="password" class="form-label fw-medium mb-0">Password</label>
			<a href={ROUTES.FORGOT_PASSWORD} class="small text-decoration-none">Forgot password?</a>
		</div>
		<div class="fr-password-wrap">
			<input
				type={showPassword ? 'text' : 'password'}
				id="password"
				name="password"
				class="form-control"
				required
				autocomplete="current-password"
				minlength="8"
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
	</div>

	<button type="submit" class="btn btn-primary w-100" disabled={loading}>
		{#if loading}
			<span class="spinner-border spinner-border-sm me-2" role="status" aria-hidden="true"></span>
		{/if}
		Sign in
	</button>
</form>

<div class="fr-auth-divider">or</div>

<p class="text-center mb-0 small">
	Don't have an account? <a href={ROUTES.REGISTER} class="fw-medium">Create one</a>
</p>
