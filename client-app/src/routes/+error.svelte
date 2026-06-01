<svelte:head>
	<title>Error {$page.status} | Ferum Board</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<script lang="ts">
	import { page } from '$app/stores';
	import { ROUTES } from '$lib/routes';

	type ErrorConfig = { icon: string; title: string; description: string; color: string };

	const ERROR_CONFIG: Record<number, ErrorConfig> = {
		400: {
			icon: 'fa-triangle-exclamation',
			title: 'Bad Request',
			description: 'The request contained invalid parameters or malformed data.',
			color: '#f59e0b',
		},
		403: {
			icon: 'fa-lock',
			title: 'Access Denied',
			description: "You don't have permission to view this page.",
			color: '#f59e0b',
		},
		404: {
			icon: 'fa-compass',
			title: 'Page Not Found',
			description: "The page you're looking for doesn't exist or may have been moved.",
			color: 'var(--bs-primary)',
		},
		500: {
			icon: 'fa-server',
			title: 'Server Error',
			description: 'Something went wrong on our end. Please try again in a moment.',
			color: '#ef4444',
		},
	};

	const config = $derived<ErrorConfig>(
		ERROR_CONFIG[$page.status] ?? {
			icon: 'fa-circle-exclamation',
			title: 'Something Went Wrong',
			description: $page.error?.message ?? 'An unexpected error occurred.',
			color: 'var(--bs-primary)',
		}
	);
</script>

<div class="fr-err-shell">
	<header class="fr-err-header">
		<a href={ROUTES.HOME} class="fr-err-brand">
			<i class="fa-solid fa-layer-group"></i>
			Ferum Board
		</a>
	</header>

	<main class="fr-err-body">
		<div class="fr-err-card">
			<div class="fr-err-watermark" aria-hidden="true">{$page.status}</div>

			<div class="fr-err-icon-wrap" style="--err-color: {config.color}">
				<i class="fa-solid {config.icon}"></i>
			</div>

			<h1 class="fr-err-title">{config.title}</h1>
			<p class="fr-err-desc">{config.description}</p>

			<div class="fr-err-actions">
				<button type="button" class="btn btn-outline-secondary" onclick={() => history.back()}>
					<i class="fa-solid fa-arrow-left me-2"></i>Go back
				</button>
				<a href={ROUTES.HOME} class="btn btn-primary">
					<i class="fa-solid fa-house me-2"></i>Home
				</a>
			</div>

			{#if $page.status === 404}
				<div class="fr-err-suggestions">
					<span>Try</span>
					<a href={ROUTES.SEARCH}>search</a>
					<span>or</span>
					<a href={ROUTES.HOME}>browse discussions</a>
				</div>
			{/if}
		</div>
	</main>
</div>

<style>
	.fr-err-shell {
		min-height: 100dvh;
		display: flex;
		flex-direction: column;
		background: var(--bs-body-bg);
	}

	.fr-err-header {
		height: var(--fr-topbar-height);
		border-bottom: 1px solid var(--bs-border-color);
		display: flex;
		align-items: center;
		padding: 0 1.5rem;
		flex-shrink: 0;
	}

	.fr-err-brand {
		display: inline-flex;
		align-items: center;
		gap: 0.5rem;
		font-weight: 700;
		font-size: 0.9375rem;
		color: var(--bs-body-color);
		text-decoration: none;
		transition: color 0.1s;
	}
	.fr-err-brand:hover { color: var(--bs-primary); }
	.fr-err-brand i { color: var(--bs-primary); }

	.fr-err-body {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 2rem 1rem;
	}

	.fr-err-card {
		position: relative;
		text-align: center;
		max-width: 480px;
		width: 100%;
		padding: 3rem 2rem 2.5rem;
		overflow: hidden;
	}

	.fr-err-watermark {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		font-size: clamp(8rem, 20vw, 13rem);
		font-weight: 900;
		letter-spacing: -0.06em;
		line-height: 1;
		color: var(--bs-border-color);
		opacity: 0.6;
		pointer-events: none;
		user-select: none;
	}

	.fr-err-icon-wrap {
		position: relative;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 5rem;
		height: 5rem;
		border-radius: 50%;
		background: color-mix(in srgb, var(--err-color) 10%, transparent);
		border: 1px solid color-mix(in srgb, var(--err-color) 22%, transparent);
		font-size: 1.75rem;
		color: var(--err-color);
		margin-bottom: 1.25rem;
	}

	.fr-err-title {
		position: relative;
		font-size: 1.375rem;
		font-weight: 700;
		color: var(--bs-body-color);
		margin: 0 0 0.5rem;
		letter-spacing: -0.02em;
	}

	.fr-err-desc {
		position: relative;
		font-size: 0.9375rem;
		color: var(--bs-secondary-color);
		line-height: 1.65;
		margin: 0 auto 2rem;
		max-width: 340px;
	}

	.fr-err-actions {
		position: relative;
		display: flex;
		gap: 0.625rem;
		justify-content: center;
		flex-wrap: wrap;
	}

	.fr-err-suggestions {
		position: relative;
		margin-top: 1.5rem;
		font-size: 0.8125rem;
		color: var(--bs-tertiary-color);
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 0.3rem;
		flex-wrap: wrap;
	}
	.fr-err-suggestions a {
		color: var(--bs-primary);
		text-decoration: none;
	}
	.fr-err-suggestions a:hover {
		text-decoration: underline;
	}
</style>
