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
			description: 'The request contained invalid parameters.',
			color: '#f59e0b',
		},
		403: {
			icon: 'fa-lock',
			title: 'Access Denied',
			description: "You don't have permission to access this page.",
			color: '#f59e0b',
		},
		404: {
			icon: 'fa-compass',
			title: 'Page Not Found',
			description: "This page doesn't exist or the link may have expired.",
			color: 'var(--bs-primary)',
		},
		500: {
			icon: 'fa-server',
			title: 'Server Error',
			description: 'Something went wrong. Please try again in a moment.',
			color: '#ef4444',
		},
	};

	const FILE_TOO_LARGE: ErrorConfig = {
		icon: 'fa-file-circle-exclamation',
		title: 'File Too Large',
		description: 'The uploaded file exceeds the maximum allowed size. Please choose a smaller file and try again.',
		color: '#f59e0b',
	};

	const config = $derived<ErrorConfig>(
		$page.status === 413 || ($page.status === 400 && ($page.error?.message ?? '').includes('multipart'))
			? FILE_TOO_LARGE
			: ERROR_CONFIG[$page.status] ?? {
				icon: 'fa-circle-exclamation',
				title: 'Something Went Wrong',
				description: 'An unexpected error occurred.',
				color: 'var(--bs-primary)',
			}
	);
</script>

<div class="text-center py-2">
	<div
		class="d-inline-flex align-items-center justify-content-center rounded-circle mb-3 err-icon"
		style="--err-color: {config.color}"
	>
		<i class="fa-solid {config.icon}"></i>
	</div>

	<div class="text-muted fw-bold mb-1 err-label">
		Error {$page.status}
	</div>
	<h1 class="h5 fw-bold mb-2 err-title">{config.title}</h1>
	<p class="text-secondary mb-4 err-desc">
		{config.description}
	</p>

	<div class="d-flex gap-2 justify-content-center flex-wrap">
		<button type="button" class="btn btn-outline-secondary btn-sm err-btn" onclick={() => history.back()}>
			<i class="fa-solid fa-arrow-left me-2"></i>Go back
		</button>
		<a href={ROUTES.LOGIN} class="btn btn-primary btn-sm err-btn">
			<i class="fa-solid fa-right-to-bracket me-2"></i>Sign in
		</a>
	</div>
</div>

<style>
	.err-icon {
		width: 3.5rem; height: 3.5rem; font-size: 1.25rem;
		background: color-mix(in srgb, var(--err-color) 10%, transparent);
		border: 1px solid color-mix(in srgb, var(--err-color) 22%, transparent);
		color: var(--err-color);
	}
	.err-label { font-size: 0.7rem; letter-spacing: 0.08em; text-transform: uppercase; }
	.err-title { letter-spacing: -0.02em; }
	.err-desc  { font-size: 0.9rem; line-height: 1.6; }
	.err-btn   { min-height: var(--fr-tap-target); padding: 0 1rem; }
</style>
