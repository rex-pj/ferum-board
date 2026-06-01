<script lang="ts">
	import '../../app.css';
	import { ROUTES } from '$lib/routes';

	let { data, children }: { data: any; children: any } = $props();
</script>

<div class="fr-auth-bg min-vh-100 d-flex align-items-center justify-content-center py-5">
	<div class="w-100 px-3" style="max-width: 440px;">
		<div class="text-center mb-4">
			<a href={ROUTES.HOME} class="text-decoration-none d-inline-flex flex-column align-items-center gap-2">
				{#if data?.logoUrl}
					<img src={data.logoUrl} alt={data?.siteName ?? 'Ferum Board'} style="height: 52px; width: auto; object-fit: contain;" />
				{:else}
					<div class="fr-auth-logo">
						<i class="fa-solid fa-comments"></i>
					</div>
				{/if}
				<span class="fw-bold fs-6 text-body" style="letter-spacing: -0.02em;">{data?.siteName ?? 'Ferum Board'}</span>
			</a>
		</div>

		<div class="card fr-auth-card">
			<div class="card-body p-4 p-sm-5">
				{@render children()}
			</div>
		</div>
	</div>
</div>
