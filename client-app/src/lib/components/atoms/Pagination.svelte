<script lang="ts">
	interface Props {
		currentPage: number;
		totalPages: number;
		buildHref: (page: number) => string;
		navClass?: string;
		compact?: boolean;
	}

	let { currentPage, totalPages, buildHref, navClass = 'mt-4', compact = false }: Props = $props();

	// Build the visible page list with ellipsis gaps.
	// Always shows: first, last, current±2, and "..." markers between gaps.
	const pages = $derived((): Array<number | null> => {
		if (totalPages <= 1) return [];

		const window = 2;
		const shown = new Set<number>();
		shown.add(1);
		shown.add(totalPages);
		for (let i = Math.max(1, currentPage - window); i <= Math.min(totalPages, currentPage + window); i++) {
			shown.add(i);
		}

		const sorted = Array.from(shown).sort((a, b) => a - b);
		const result: Array<number | null> = [];
		for (let i = 0; i < sorted.length; i++) {
			if (i > 0 && sorted[i] - sorted[i - 1] > 1) {
				result.push(null); // ellipsis marker
			}
			result.push(sorted[i]);
		}
		return result;
	});
</script>

{#if totalPages > 1}
	<nav aria-label="Pagination" class={navClass}>
		{#if compact}
			<div class="d-flex align-items-center gap-2" style="font-size:0.8125rem; color:var(--bs-secondary-color);">
				{#if currentPage > 1}
					<a href={buildHref(currentPage - 1)} class="fr-page-link" aria-label="Previous page">
						<i class="fa-solid fa-chevron-left" style="font-size:0.6875rem;"></i> Prev
					</a>
					<span style="opacity:0.35;">|</span>
				{/if}
				<span>Page <strong style="color:var(--bs-body-color);">{currentPage}</strong> of {totalPages}</span>
				{#if currentPage < totalPages}
					<span style="opacity:0.35;">|</span>
					<a href={buildHref(currentPage + 1)} class="fr-page-link" aria-label="Next page">
						Next <i class="fa-solid fa-chevron-right" style="font-size:0.6875rem;"></i>
					</a>
				{/if}
			</div>
		{:else}
			<ul class="pagination pagination-sm flex-wrap justify-content-center gap-1 mb-0">
				<!-- Previous -->
				<li class="page-item" class:disabled={currentPage <= 1}>
					<a
						class="page-link"
						href={currentPage > 1 ? buildHref(currentPage - 1) : undefined}
						aria-label="Previous page"
						aria-disabled={currentPage <= 1}
					>
						<i class="fa-solid fa-chevron-left"></i>
					</a>
				</li>

				{#each pages() as p}
					{#if p === null}
						<li class="page-item disabled" aria-hidden="true">
							<span class="page-link" style="pointer-events:none;">…</span>
						</li>
					{:else}
						<li class="page-item" class:active={p === currentPage}>
							<a class="page-link" href={buildHref(p)} aria-current={p === currentPage ? 'page' : undefined}>
								{p}
							</a>
						</li>
					{/if}
				{/each}

				<!-- Next -->
				<li class="page-item" class:disabled={currentPage >= totalPages}>
					<a
						class="page-link"
						href={currentPage < totalPages ? buildHref(currentPage + 1) : undefined}
						aria-label="Next page"
						aria-disabled={currentPage >= totalPages}
					>
						<i class="fa-solid fa-chevron-right"></i>
					</a>
				</li>
			</ul>
		{/if}
	</nav>
{/if}

<style>
	.fr-page-link {
		color: var(--bs-secondary-color);
		text-decoration: none;
		transition: color 0.15s;
	}
	.fr-page-link:hover {
		color: var(--bs-body-color);
	}
</style>
