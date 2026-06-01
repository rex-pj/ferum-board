<svelte:head>
	<title>Create an account | Ferum Board</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<script lang="ts">
	import { enhance } from '$app/forms';
	import { ROUTES } from '$lib/routes';

	let { form }: { form: any } = $props();
	let loading = $state(false);
	let showPassword = $state(false);
</script>

{#if form?.success}
	<div class="text-center py-3">
		<div class="fr-auth-icon-circle mx-auto mb-3" style="background: color-mix(in srgb, #10b981 10%, var(--bs-body-bg)); border-color: color-mix(in srgb, #10b981 25%, var(--bs-border-color)); color: #10b981;">
			<i class="fa-solid fa-envelope-circle-check"></i>
		</div>
		<h2 class="fr-auth-heading mb-1">Check your inbox</h2>
		<p class="fr-auth-sub mb-4">We sent a verification link to your email address. Click it to activate your account, then sign in.</p>
		<a href={ROUTES.LOGIN} class="btn btn-primary w-100">Go to sign in</a>
	</div>
{:else}
	<div class="mb-4">
		<h1 class="fr-auth-heading">Create your account</h1>
		<p class="fr-auth-sub">Join the community. It's free.</p>
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
			<label for="username" class="form-label fw-medium">Username</label>
			<input
				type="text"
				id="username"
				name="username"
				class="form-control"
				required
				minlength="3"
				maxlength="30"
				pattern={"[a-zA-Z0-9_\\-]+"}
				autocomplete="username"
				value={form?.username ?? ''}
			/>
			<div class="form-text">3–30 characters. Letters, numbers, _ and - only.</div>
		</div>

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
			<label for="password" class="form-label fw-medium">Password</label>
			<div class="fr-password-wrap">
				<input
					type={showPassword ? 'text' : 'password'}
					id="password"
					name="password"
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
			Create account
		</button>
	</form>

	<div class="fr-auth-divider">or</div>

	<p class="text-center mb-0 small">
		Already have an account? <a href={ROUTES.LOGIN} class="fw-medium">Sign in</a>
	</p>
{/if}
