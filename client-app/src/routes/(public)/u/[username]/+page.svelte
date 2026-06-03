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

	const TRUST_LABELS: Record<string, string> = {
		new:     'New',
		basic:   'Basic',
		member:  'Member',
		regular: 'Regular',
		leader:  'Leader',
	};

	const TRUST_DESCRIPTIONS: Record<string, string> = {
		new:     'New member — getting started',
		basic:   'Basic member — verified and active',
		member:  'Trusted member of the community',
		regular: 'Regular contributor',
		leader:  'Community leader with elevated trust',
	};

	// Matches the hue derivation in Avatar.svelte for a consistent per-user colour
	const profileHue = profile
		? profile.username.split('').reduce((acc: number, c: string) => acc + c.charCodeAt(0), 0) % 360
		: 0;
</script>

<div class="fr-content-layout">

<!-- ── Main column ─────────────────────────────────────────── -->
<div class="fr-feed-col">
	{#if profile}

		<!-- Profile hero card -->
		<div class="card mb-4 fr-profile-card">
			<!-- Gradient banner -->
			<div class="fr-profile-banner" style="--profile-hue:{profileHue}"></div>

			<!-- Avatar: absolutely positioned so it is never in the card's flex flow
			     and can never be clipped by sibling paint order -->
			<div class="fr-profile-avatar-abs">
				<Avatar src={profile.avatar_url} username={profile.username} size={80} />
			</div>

			<!-- Name, badges, bio, stats — padding-top clears the avatar overlap -->
			<div class="px-3 px-sm-4 pb-3 pb-sm-4 fr-profile-body">
				<!-- Name + Edit Profile in the same row -->
				<div class="d-flex align-items-start justify-content-between gap-2 flex-wrap mb-1">
					<h1 class="h4 fw-bold mb-0">{profile.display_name ?? profile.username}</h1>
					{#if data.user?.username === profile.username}
						<a href={ROUTES.ACCOUNT} class="btn btn-outline-secondary btn-sm flex-shrink-0">
							<i class="fa-solid fa-pen me-1"></i>Edit Profile
						</a>
					{/if}
				</div>

				<div class="d-flex align-items-center gap-2 flex-wrap mb-2">
					<span class="text-muted small">@{profile.username}</span>
					{#if profile.primary_role_slug && profile.primary_role_slug !== 'member'}
						<Badge role={profile.primary_role_slug} />
					{/if}
					<Badge trust={profile.trust_level} label={TRUST_LABELS[profile.trust_level] ?? profile.trust_level} />
				</div>

				{#if profile.bio}
					<p class="text-body-secondary small lh-base mb-1">{profile.bio}</p>
				{/if}
				{#if profile.website}
					<a href={profile.website} rel="nofollow noopener" class="small text-break d-inline-flex align-items-center gap-1 mb-1">
						<i class="fa-solid fa-link fa-xs"></i>{profile.website}
					</a>
				{/if}

				<!-- Stats strip -->
				<div class="fr-profile-stats mt-3">
					<div class="fr-profile-stat-item">
						<span class="fw-semibold small text-body">{(profile.post_count ?? 0).toLocaleString()}</span>
						<span class="fr-stat-label">Posts</span>
					</div>
					{#if profile.trust_score != null}
						<div class="fr-profile-stat-item">
							<span class="fw-semibold small text-body">
								<i class="fa-solid fa-star fa-xs text-warning"></i> {profile.trust_score}
							</span>
							<span class="fr-stat-label">Trust</span>
						</div>
					{/if}
					<div class="fr-profile-stat-item">
						<span class="fw-semibold small text-body"><Timestamp date={profile.created_at} /></span>
						<span class="fr-stat-label">Joined</span>
					</div>
				</div>
			</div>
		</div>

		<!-- Thread list -->
		<div class="d-flex align-items-center mb-3">
			<h2 class="h6 fw-semibold mb-0" style="color:var(--bs-secondary-color)">
				<i class="fa-regular fa-comment-dots me-2"></i>Threads
				{#if data.threadTotal > 0}
					<span class="badge bg-secondary ms-1 fw-normal" style="font-size:0.7rem">{data.threadTotal.toLocaleString()}</span>
				{/if}
			</h2>
		</div>

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
				<h2 class="fr-empty-title">No threads yet</h2>
				<p class="fr-empty-sub">This user hasn't started any threads yet.</p>
			</div>
		{/if}

	{:else}
		<div class="alert alert-warning">User not found.</div>
	{/if}
</div>

<!-- ── Right panel ──────────────────────────────────────────── -->
{#if profile}
<aside class="fr-right-panel">

	<!-- Stats card -->
	<div class="fr-panel">
		<div class="fr-panel-header">Stats</div>
		<div class="fr-panel-stat">
			<span class="text-muted small"><i class="fa-regular fa-comment me-2 opacity-50"></i>Posts</span>
			<span class="fw-semibold small">{(profile.post_count ?? 0).toLocaleString()}</span>
		</div>
		{#if profile.trust_score != null}
			<div class="fr-panel-stat">
				<span class="text-muted small"><i class="fa-solid fa-star me-2 text-warning opacity-75"></i>Trust score</span>
				<span class="fw-semibold small">{profile.trust_score}</span>
			</div>
		{/if}
		<div class="fr-panel-stat">
			<span class="text-muted small"><i class="fa-regular fa-calendar me-2 opacity-50"></i>Joined</span>
			<span class="fw-semibold small"><Timestamp date={profile.created_at} /></span>
		</div>
	</div>

	<!-- Member status card -->
	<div class="fr-panel">
		<div class="fr-panel-header">Member status</div>
		<div class="px-3 py-3 d-flex flex-column gap-2">
			<div class="d-flex align-items-center justify-content-between">
				<span class="text-muted small">Trust level</span>
				<Badge trust={profile.trust_level} label={TRUST_LABELS[profile.trust_level] ?? profile.trust_level} />
			</div>
			{#if profile.primary_role_slug && profile.primary_role_slug !== 'member'}
				<div class="d-flex align-items-center justify-content-between">
					<span class="text-muted small">Role</span>
					<Badge role={profile.primary_role_slug} />
				</div>
			{/if}
			{#if TRUST_DESCRIPTIONS[profile.trust_level]}
				<p class="mb-0 mt-1 lh-base" style="font-size:0.75rem;color:var(--bs-secondary-color)">
					{TRUST_DESCRIPTIONS[profile.trust_level]}
				</p>
			{/if}
		</div>
	</div>

</aside>
{/if}

</div>

<style>
	/* ── Banner ───────────────────────────────────────────────── */
	.fr-profile-banner {
		height: 88px;
		background: linear-gradient(
			135deg,
			hsl(calc(var(--profile-hue) + 20), 55%, 38%) 0%,
			hsl(var(--profile-hue), 65%, 52%) 100%
		);
		border-top-left-radius: 7px; /* matches card border-radius minus border */
		border-top-right-radius: 7px;
	}

	:global([data-bs-theme='dark']) .fr-profile-banner {
		filter: brightness(0.55) saturate(1.3);
	}

	/* ── Avatar (absolute — never in flex flow, never clipped by siblings) ── */
	.fr-profile-card {
		position: relative; /* Bootstrap already sets this; explicit for clarity */
	}

	.fr-profile-avatar-abs {
		position: absolute;
		left: 1.25rem;
		/* banner = 88px, avatar = 80px → center avatar on banner bottom edge */
		top: calc(88px - 40px); /* = 48px */
		width: 80px;
		height: 80px;
		border-radius: 50%;
		overflow: hidden;
		line-height: 0;
		z-index: 1;
		box-shadow: 0 0 0 3px var(--bs-body-bg), 0 2px 8px rgba(0, 0, 0, 0.25);
	}

	/* Content area: top padding = avatar overlap (40px) + gap (12px) */
	.fr-profile-body {
		padding-top: 52px;
	}

	/* ── Stats strip ──────────────────────────────────────────── */
	.fr-profile-stats {
		display: flex;
		border: 1px solid var(--bs-border-color);
		border-radius: var(--bs-border-radius-sm);
		overflow: hidden;
	}

	.fr-profile-stat-item {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 0.1rem;
		padding: 0.5rem 0.375rem;
	}

	.fr-profile-stat-item + .fr-profile-stat-item {
		border-left: 1px solid var(--bs-border-color);
	}

	.fr-stat-label {
		font-size: 0.6875rem;
		color: var(--bs-secondary-color);
		text-transform: uppercase;
		letter-spacing: 0.05em;
		font-weight: 500;
	}
</style>
