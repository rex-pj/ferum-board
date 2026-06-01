<svelte:head>
	<title>{data.profile?.display_name ?? data.profile?.username ?? 'User'} | {data.siteName ?? 'Ferum Board'}</title>
	<meta name="description" content={data.profile?.bio ?? `${data.profile?.username}'s profile on Ferum Board`} />
	<meta property="og:title" content="{data.profile?.display_name ?? data.profile?.username ?? 'User'} | {data.siteName ?? 'Ferum Board'}" />
	<meta property="og:description" content={data.profile?.bio ?? `${data.profile?.username}'s profile on Ferum Board`} />
	<meta property="og:type" content="profile" />
</svelte:head>

<script lang="ts">
	import Avatar from '$lib/components/atoms/Avatar.svelte';
	import Badge from '$lib/components/atoms/Badge.svelte';
	import Timestamp from '$lib/components/atoms/Timestamp.svelte';
	import ThreadCard from '$lib/components/molecules/ThreadCard.svelte';
	import { ROUTES } from '$lib/routes';

	let { data }: { data: any } = $props();

	const profile = data.profile;

	const TRUST_DESCRIPTIONS: Record<string, string> = {
		new:    'New member — getting started',
		basic:  'Basic member — verified and active',
		member: 'Trusted member of the community',
		leader: 'Community leader with elevated trust',
	};
</script>

<div class="fr-content-layout">
<!-- Profile main column -->
<div class="fr-feed-col">
	{#if profile}
		<!-- Profile header -->
		<div class="d-flex align-items-start gap-3 gap-sm-4 mb-4">
			<Avatar src={profile.avatar_url} username={profile.username} size={80} />
			<div class="flex-grow-1 min-w-0">
				<div class="d-flex align-items-start justify-content-between gap-2 flex-wrap">
					<div>
						<h1 class="h4 mb-1 text-truncate">{profile.display_name ?? profile.username}</h1>
						<div class="d-flex align-items-center gap-2 flex-wrap">
							<span class="text-muted small">@{profile.username}</span>
							{#if profile.role && profile.role !== 'member'}
								<Badge role={profile.role} />
							{/if}
							{#if profile.is_global_mod}
								<span class="badge bg-warning text-dark">Global Mod</span>
							{/if}
						</div>
					</div>
					{#if data.user?.username === profile.username}
						<a href={ROUTES.ACCOUNT} class="btn btn-outline-secondary btn-sm flex-shrink-0">
							<i class="fa-solid fa-pen me-1"></i>Edit Profile
						</a>
					{/if}
				</div>

				<!-- Meta row: joined + post count -->
				<div class="d-flex align-items-center gap-3 flex-wrap mt-2 small text-muted">
					<span><i class="fa-regular fa-calendar me-1"></i>Joined <Timestamp date={profile.created_at} /></span>
					<span><i class="fa-regular fa-comment me-1"></i>{(profile.post_count ?? 0).toLocaleString()} posts</span>
				</div>

				{#if profile.bio}
					<p class="mb-0 mt-2" style="color: var(--bs-body-color);">{profile.bio}</p>
				{/if}
				{#if profile.website}
					<a href={profile.website} rel="nofollow noopener" class="small mt-1 d-inline-block text-break">
						<i class="fa-solid fa-link me-1"></i>{profile.website}
					</a>
				{/if}
			</div>
		</div>

		<!-- Divider -->
		<hr class="my-4" />

		<!-- Thread list -->
		{#if data.threads && data.threads.length > 0}
			<div class="d-flex flex-column gap-3">
				{#each data.threads as thread (thread.id)}
					<ThreadCard {thread} />
				{/each}
			</div>

			{#if data.threadTotal > data.threadPerPage}
				{@const totalPages = Math.ceil(data.threadTotal / data.threadPerPage)}
				<nav class="mt-4" aria-label="Thread pagination">
					<ul class="pagination justify-content-center">
						{#if data.threadPage > 1}
							<li class="page-item">
								<a class="page-link" href="{ROUTES.USER_PROFILE(profile.username)}?page={data.threadPage - 1}">
									<i class="fa-solid fa-chevron-left"></i>
								</a>
							</li>
						{/if}
						{#each Array.from({ length: totalPages }, (_, i) => i + 1) as p}
							<li class="page-item" class:active={p === data.threadPage}>
								<a class="page-link" href="{ROUTES.USER_PROFILE(profile.username)}?page={p}">{p}</a>
							</li>
						{/each}
						{#if data.threadPage < totalPages}
							<li class="page-item">
								<a class="page-link" href="{ROUTES.USER_PROFILE(profile.username)}?page={data.threadPage + 1}">
									<i class="fa-solid fa-chevron-right"></i>
								</a>
							</li>
						{/if}
					</ul>
				</nav>
			{/if}
		{:else}
			<div class="fr-empty-state">
				<div class="fr-empty-icon">
					<i class="fa-regular fa-comment-dots"></i>
				</div>
				<h2 class="fr-empty-title">No posts yet</h2>
				<p class="fr-empty-sub">This user hasn't posted anything yet.</p>
			</div>
		{/if}
	{:else}
		<div class="alert alert-warning">User not found.</div>
	{/if}
</div>

<!-- Right panel -->
{#if profile}
<aside class="fr-right-panel">
	<!-- Trust level + score combined -->
	<div class="fr-panel">
		<div class="fr-panel-header">Trust Level</div>
		<div class="px-3 py-3">
			<div class="d-flex align-items-center justify-content-between mb-2">
				<Badge trust={profile.trust_level} label={profile.trust_level} />
				{#if profile.trust_score != null}
					<span class="small fw-semibold" style="color: var(--bs-body-color);">
						<i class="fa-solid fa-star me-1 text-warning"></i>{profile.trust_score} pts
					</span>
				{/if}
			</div>
			{#if TRUST_DESCRIPTIONS[profile.trust_level]}
				<p class="small mb-0 text-muted" style="line-height: 1.5;">
					{TRUST_DESCRIPTIONS[profile.trust_level]}
				</p>
			{/if}
		</div>
	</div>

	<!-- Role (non-member only) -->
	{#if profile.role && profile.role !== 'member'}
		<div class="fr-panel">
			<div class="fr-panel-header">Role</div>
			<div class="px-3 py-3">
				<Badge role={profile.role} />
				{#if profile.is_global_mod}
					<p class="small mt-2 mb-0 text-muted">Global moderator across all categories.</p>
				{/if}
			</div>
		</div>
	{/if}
</aside>
{/if}
</div>
