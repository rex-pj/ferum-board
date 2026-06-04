<script lang="ts">
	import '../../app.css';
	import { ROUTES } from '$lib/routes';

	let { data, children }: { data: any; children: any } = $props();
</script>

<div class="fr-auth-bg min-vh-100 d-flex align-items-center justify-content-center py-5">
	<div class="w-100 px-3 auth-wrapper">
		<div class="text-center mb-4">
			<a href={ROUTES.HOME} class="text-decoration-none d-inline-flex auth-logo-link" aria-label={data?.siteName ?? 'Ferum Board'}>
				{#if data?.logoUrl}
					<img src={data.logoUrl} alt={data.siteName ?? 'Ferum Board'} class="auth-logo-img" />
				{:else}
					<svg viewBox="0 0 160 36" height="34" aria-hidden="true" focusable="false" overflow="visible">
						<text x="0" y="28" font-family="'Inter', 'Helvetica Neue', Arial, sans-serif" font-size="30" font-weight="800" letter-spacing="-1" fill="var(--bs-link-color)">Fe</text>
						<text x="36" y="28" font-family="'Inter', 'Helvetica Neue', Arial, sans-serif" font-size="30" font-weight="800" letter-spacing="-1" fill="var(--bs-body-color)">rum</text>
						<text x="102" y="28" font-family="'Inter', 'Helvetica Neue', Arial, sans-serif" font-size="20" font-weight="400" fill="var(--bs-link-color)">board</text>
					</svg>
				{/if}
			</a>
			{#if data?.siteSlogan}
				<p class="auth-slogan">{data.siteSlogan}</p>
			{/if}
		</div>

		<div class="card fr-auth-card">
			<div class="card-body p-4 p-sm-5">
				{@render children()}
			</div>
		</div>
	</div>
</div>

<style>
	.auth-wrapper   { max-width: 440px; }
	.auth-logo-img  { height: 34px; width: auto; object-fit: contain; }
	.auth-logo-link { color: inherit; }
	.auth-logo-link:hover { color: inherit; }
	.auth-slogan {
		font-size: 0.8125rem;
		color: var(--bs-secondary-color);
		margin: 0.375rem 0 0;
	}
</style>
