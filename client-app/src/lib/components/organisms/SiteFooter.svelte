<script lang="ts">
	import { ROUTES } from '$lib/routes';

	interface Props {
		siteName?: string;
		siteSlogan?: string | null;
		logoUrl?: string | null;
		compact?: boolean;
	}

	let { siteName = 'Ferum Board', siteSlogan = null, logoUrl = null, compact = false }: Props = $props();

	const year = new Date().getFullYear();
</script>

{#if compact}
	<!-- Compact: single bar for admin / mod pages -->
	<footer class="fr-footer border-top mt-auto">
		<div class="container-fluid px-4">
			<div class="fr-footer-compact">
				<div class="d-flex align-items-center gap-2">
					<a href={ROUTES.HOME} class="fr-brand-link" aria-label={siteName}>
						{#if logoUrl}
							<img src={logoUrl} alt={siteName} class="fr-footer-logo-img" />
						{:else}
							<svg viewBox="0 0 160 36" height="22" aria-hidden="true" focusable="false" overflow="visible">
								<text x="0" y="28" font-family="'Inter', 'Helvetica Neue', Arial, sans-serif" font-size="30" font-weight="800" letter-spacing="-1" fill="var(--bs-link-color)">Fe</text>
								<text x="36" y="28" font-family="'Inter', 'Helvetica Neue', Arial, sans-serif" font-size="30" font-weight="800" letter-spacing="-1" fill="currentColor">rum</text>
								<text x="102" y="28" font-family="'Inter', 'Helvetica Neue', Arial, sans-serif" font-size="20" font-weight="400" fill="var(--bs-link-color)">board</text>
							</svg>
						{/if}
					</a>
					{#if siteSlogan}
						<span class="fr-footer-compact-slogan">{siteSlogan}</span>
					{/if}
				</div>
				<span class="fr-footer-copy">© {year} {siteName}. Powered by Ferum Board.</span>
			</div>
		</div>
	</footer>
{:else}
	<!-- Full footer: public pages -->
	<footer class="fr-footer border-top mt-auto">
		<div class="container-xl px-3 px-lg-4">

			<div class="fr-footer-grid py-4">

				<!-- Brand -->
				<div class="fr-footer-brand-col">
					<a href={ROUTES.HOME} class="fr-brand-link" aria-label={siteName}>
						{#if logoUrl}
							<img src={logoUrl} alt={siteName} class="fr-footer-logo-img" />
						{:else}
							<svg viewBox="0 0 160 36" height="26" aria-hidden="true" focusable="false" overflow="visible">
								<text x="0" y="28" font-family="'Inter', 'Helvetica Neue', Arial, sans-serif" font-size="30" font-weight="800" letter-spacing="-1" fill="var(--bs-link-color)">Fe</text>
								<text x="36" y="28" font-family="'Inter', 'Helvetica Neue', Arial, sans-serif" font-size="30" font-weight="800" letter-spacing="-1" fill="currentColor">rum</text>
								<text x="102" y="28" font-family="'Inter', 'Helvetica Neue', Arial, sans-serif" font-size="20" font-weight="400" fill="var(--bs-link-color)">board</text>
							</svg>
						{/if}
					</a>
					{#if siteSlogan}
						<p class="fr-footer-tagline">{siteSlogan}</p>
					{/if}
				</div>

				<!-- Browse -->
				<div>
					<h6 class="fr-label-caps fr-footer-heading">Browse</h6>
					<ul class="fr-footer-links">
						<li>
							<a href={ROUTES.HOME} class="fr-footer-link">
								<i class="fa-solid fa-house fa-fw"></i> Home Feed
							</a>
						</li>
						<li>
							<a href={ROUTES.FORUM_INDEX} class="fr-footer-link">
								<i class="fa-solid fa-layer-group fa-fw"></i> All Categories
							</a>
						</li>
						<li>
							<a href={ROUTES.SEARCH} class="fr-footer-link">
								<i class="fa-solid fa-magnifying-glass fa-fw"></i> Search
							</a>
						</li>
					</ul>
				</div>

				<!-- Account -->
				<div>
					<h6 class="fr-label-caps fr-footer-heading">Account</h6>
					<ul class="fr-footer-links">
						<li>
							<a href={ROUTES.LOGIN} class="fr-footer-link">
								<i class="fa-solid fa-right-to-bracket fa-fw"></i> Sign In
							</a>
						</li>
						<li>
							<a href={ROUTES.REGISTER} class="fr-footer-link">
								<i class="fa-solid fa-user-plus fa-fw"></i> Create Account
							</a>
						</li>
						<li>
							<a href={ROUTES.FORGOT_PASSWORD} class="fr-footer-link">
								<i class="fa-solid fa-key fa-fw"></i> Forgot Password
							</a>
						</li>
					</ul>
				</div>

			</div>

			<!-- Bottom bar -->
			<div class="fr-footer-bottom">
				<span>© {year} {siteName}. Powered by Ferum Board.</span>
				<div class="fr-footer-legal">
					<a href={ROUTES.SEARCH + '?q=privacy'} class="fr-footer-legal-link">Privacy</a>
					<span class="fr-footer-sep" aria-hidden="true">·</span>
					<a href={ROUTES.SEARCH + '?q=terms'} class="fr-footer-legal-link">Terms</a>
				</div>
			</div>

		</div>
	</footer>
{/if}

<style>
	.fr-footer {
		background: var(--bs-tertiary-bg);
		color: var(--bs-secondary-color);
	}

	.fr-footer-logo-img {
		height: 26px;
		width: auto;
		object-fit: contain;
		flex-shrink: 0;
	}

	/* ── Compact (admin / mod) ───────────────────────────── */
	.fr-footer-compact {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0.75rem 0;
		gap: 1rem;
		flex-wrap: wrap;
	}

	.fr-footer-copy {
		font-size: 0.8125rem;
		color: var(--bs-tertiary-color);
	}

	.fr-footer-compact-slogan {
		font-size: 0.75rem;
		color: var(--bs-tertiary-color);
		border-left: 1px solid var(--bs-border-color);
		padding-left: 0.5rem;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		max-width: 200px;
	}

	/* ── Full (public) ───────────────────────────────────── */
	.fr-footer-grid {
		display: grid;
		grid-template-columns: 1.6fr 1fr 1fr;
		gap: 2rem;
	}
	@media (max-width: 991.98px) {
		.fr-footer-grid { grid-template-columns: 1fr 1fr; }
		.fr-footer-brand-col { grid-column: 1 / -1; }
	}
	@media (max-width: 575.98px) {
		.fr-footer-grid { grid-template-columns: 1fr; }
		.fr-footer-brand-col { grid-column: auto; }
	}

	.fr-footer-tagline {
		font-size: 0.8125rem;
		line-height: 1.55;
		color: var(--bs-tertiary-color);
		margin: 0.375rem 0 0;
		max-width: 22rem;
	}

	.fr-footer-heading {
		margin: 0 0 0.625rem;
	}

	.fr-footer-links {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
	}

	.fr-footer-link {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		min-height: 32px;
		font-size: 0.875rem;
		color: var(--bs-secondary-color);
		text-decoration: none;
		border-radius: var(--bs-border-radius-sm);
		padding: 0 0.375rem;
		margin: 0 -0.375rem;
		transition: background 0.1s, color 0.1s;
	}
	.fr-footer-link:hover {
		color: var(--bs-body-color);
		background: var(--fr-nav-hover-bg);
	}
	.fr-footer-link :global(i) {
		font-size: 0.75rem;
		opacity: 0.7;
	}

	.fr-footer-bottom {
		display: flex;
		align-items: center;
		justify-content: space-between;
		flex-wrap: wrap;
		gap: 0.5rem;
		padding: 0.875rem 0;
		border-top: 1px solid var(--bs-border-color);
		font-size: 0.8125rem;
		color: var(--bs-tertiary-color);
	}

	.fr-footer-legal {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	.fr-footer-legal-link {
		color: var(--bs-tertiary-color);
		text-decoration: none;
		transition: color 0.1s;
	}
	.fr-footer-legal-link:hover {
		color: var(--bs-secondary-color);
		text-decoration: underline;
	}

	.fr-footer-sep { color: var(--bs-border-color); }
</style>
