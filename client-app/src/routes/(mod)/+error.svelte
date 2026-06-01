<svelte:head>
	<title>Error {$page.status} | Mod | Ferum Board</title>
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
			description: "You don't have moderator permission for this resource.",
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

	const config = $derived<ErrorConfig>(
		ERROR_CONFIG[$page.status] ?? {
			icon: 'fa-circle-exclamation',
			title: 'Something Went Wrong',
			description: $page.error?.message ?? 'An unexpected error occurred.',
			color: 'var(--bs-primary)',
		}
	);
</script>

<div class="d-flex align-items-center justify-content-center" style="min-height: 60vh;">
	<div class="text-center" style="max-width: 440px; width: 100%;">
		<div
			class="d-inline-flex align-items-center justify-content-center rounded-circle mb-4"
			style="width: 4.5rem; height: 4.5rem; background: color-mix(in srgb, {config.color} 10%, transparent); border: 1px solid color-mix(in srgb, {config.color} 22%, transparent); font-size: 1.5rem; color: {config.color};"
		>
			<i class="fa-solid {config.icon}"></i>
		</div>

		<div class="text-muted fw-bold mb-1" style="font-size: 0.75rem; letter-spacing: 0.08em; text-transform: uppercase;">
			Error {$page.status}
		</div>
		<h1 class="h4 fw-bold mb-2" style="letter-spacing: -0.02em;">{config.title}</h1>
		<p class="text-secondary mb-4" style="font-size: 0.9375rem; line-height: 1.65; max-width: 320px; margin: 0 auto 1.5rem;">
			{config.description}
		</p>

		<div class="d-flex gap-2 justify-content-center flex-wrap">
			<button type="button" class="btn btn-outline-secondary" style="min-height: 44px;" onclick={() => history.back()}>
				<i class="fa-solid fa-arrow-left me-2"></i>Go back
			</button>
			<a href={ROUTES.MOD.REPORTS} class="btn btn-primary" style="min-height: 44px;">
				<i class="fa-solid fa-flag me-2"></i>Reports
			</a>
		</div>
	</div>
</div>
