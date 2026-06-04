<svelte:head>
	<title>Error {$page.status} | Admin | Ferum Board</title>
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
			description: "You don't have permission to access this admin page.",
			color: '#f59e0b',
		},
		404: {
			icon: 'fa-compass',
			title: 'Not Found',
			description: "The resource you're looking for doesn't exist or may have been deleted.",
			color: 'var(--bs-primary)',
		},
		500: {
			icon: 'fa-server',
			title: 'Server Error',
			description: 'Something went wrong on our end. Please try again in a moment.',
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

<div class="d-flex align-items-center justify-content-center err-shell">
	<div class="text-center err-content">
		<div
			class="d-inline-flex align-items-center justify-content-center rounded-circle mb-4 err-icon"
			style="--err-color: {config.color}"
		>
			<i class="fa-solid {config.icon}"></i>
		</div>

		<div class="text-muted fw-bold mb-1 err-label">
			Error {$page.status}
		</div>
		<h1 class="h4 fw-bold mb-2 err-title">{config.title}</h1>
		<p class="text-secondary err-desc">
			{config.description}
		</p>

		<div class="d-flex gap-2 justify-content-center flex-wrap">
			<button type="button" class="btn btn-outline-secondary" onclick={() => history.back()}>
				<i class="fa-solid fa-arrow-left me-2"></i>Go back
			</button>
			<a href={ROUTES.ADMIN.DASHBOARD} class="btn btn-primary">
				<i class="fa-solid fa-gauge me-2"></i>Dashboard
			</a>
		</div>
	</div>
</div>

<style>
	.err-shell {
		min-height: 60vh;
	}

	.err-content {
		max-width: 440px;
		width: 100%;
	}

	.err-icon {
		width: 4.5rem;
		height: 4.5rem;
		font-size: 1.5rem;
		background: color-mix(in srgb, var(--err-color) 10%, transparent);
		border: 1px solid color-mix(in srgb, var(--err-color) 22%, transparent);
		color: var(--err-color);
	}

	.err-label {
		font-size: 0.75rem;
		letter-spacing: 0.08em;
		text-transform: uppercase;
	}

	.err-title {
		letter-spacing: -0.02em;
	}

	.err-desc {
		font-size: 0.9375rem;
		line-height: 1.65;
		max-width: 320px;
		margin: 0 auto 1.5rem;
	}
</style>
