<svelte:head>
	<title>Followers of {data.profile?.display_name ?? data.profile?.username ?? 'User'}</title>
</svelte:head>

<script lang="ts">
	import Avatar from '$lib/components/atoms/Avatar.svelte';
	import Badge from '$lib/components/atoms/Badge.svelte';
	import Timestamp from '$lib/components/atoms/Timestamp.svelte';
	import { ROUTES } from '$lib/routes';

	let { data }: { data: any } = $props();
	const profile      = $derived(data.profile);
	const followerCount = $derived(data.followStatus?.follower_count ?? 0);
	const followingCount = $derived(data.followStatus?.following_count ?? 0);
	const totalPages   = $derived(Math.ceil((data.total ?? 0) / (data.perPage ?? 20)));
	const currentPage  = $derived(data.currentPage ?? 1);
	const profileHue   = $derived(
		profile
			? profile.username.split('').reduce((acc: number, c: string) => acc + c.charCodeAt(0), 0) % 360
			: 0
	);

	const TRUST_LABELS: Record<string, string> = {
		new: 'New', basic: 'Basic', member: 'Member', regular: 'Regular', leader: 'Leader'
	};
</script>

<div class="fr-content-layout">
	<div class="fr-feed-col">

		<!-- Profile card — identical structure to the profile page -->
		<div class="card mb-4 fr-profile-card">
			{#if profile.cover_url}
				<div class="fr-profile-banner fr-profile-banner--photo" style="background-image:url('{profile.cover_url}')"></div>
			{:else}
				<div class="fr-profile-banner" style="--profile-hue:{profileHue}"></div>
			{/if}

			<div class="fr-profile-avatar-abs">
				<Avatar src={profile.avatar_url} username={profile.username} size={80} />
			</div>

			<div class="px-3 px-sm-4 pb-3 pb-sm-4 fr-profile-body">
				<div class="d-flex align-items-start justify-content-between gap-2 flex-wrap mb-1">
					<h1 class="h4 fw-bold mb-0">{profile.display_name ?? profile.username}</h1>
					<a href={ROUTES.USER_PROFILE(profile.username)} class="btn btn-outline-secondary btn-sm flex-shrink-0">
						<i class="fa-solid fa-arrow-left me-1"></i>Profile
					</a>
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

				<div class="fr-profile-stats mt-3">
					<a href={ROUTES.USER_POSTS(profile.username)} class="fr-profile-stat-item text-decoration-none text-reset">
						<span class="fw-semibold small text-body">{(profile.post_count ?? 0).toLocaleString()}</span>
						<span class="fr-stat-label">Posts</span>
					</a>
					<!-- Active: not a link since we're on the Followers page -->
					<div class="fr-profile-stat-item fr-stat-active">
						<span class="fw-semibold small" style="color:var(--bs-primary)">{followerCount.toLocaleString()}</span>
						<span class="fr-stat-label fr-stat-label-active">Followers</span>
					</div>
					<a href={ROUTES.USER_FOLLOWING(profile.username)} class="fr-profile-stat-item text-decoration-none text-reset">
						<span class="fw-semibold small text-body">{followingCount.toLocaleString()}</span>
						<span class="fr-stat-label">Following</span>
					</a>
					{#if profile.trust_score != null}
						<div class="fr-profile-stat-item">
							<span class="fw-semibold small text-body" title="Trust score: {profile.trust_score}">
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

		<!-- Section heading — same pattern as "Threads" on the profile page -->
		<div class="d-flex justify-content-between align-items-center mb-3">
			<h2 class="h6 fw-semibold mb-0" style="color:var(--bs-secondary-color)">
				<i class="fa-solid fa-users me-2"></i>Followers
				{#if data.total > 0}
					<span class="badge bg-secondary ms-1 fw-normal" style="font-size:0.7rem">{data.total.toLocaleString()}</span>
				{/if}
			</h2>
		</div>

		{#if data.followers.length > 0}
			<div class="fr-feed mb-3">
				{#each data.followers as item (item.id)}
					<a href={ROUTES.USER_PROFILE(item.user.username)} class="fr-user-row">
						<Avatar src={item.user.avatar_url} username={item.user.username} size={40} />
						<div class="fr-user-body">
							<div class="fw-medium fr-user-name">{item.user.display_name ?? item.user.username}</div>
							<div class="fr-user-meta">
								<span>@{item.user.username}</span>
								<span class="fr-user-sep">·</span>
								<span>Followed <Timestamp date={item.followed_at} /></span>
							</div>
						</div>
						<i class="fa-solid fa-chevron-right fr-user-arrow"></i>
					</a>
				{/each}
			</div>

			{#if totalPages > 1}
				<nav aria-label="Followers pages">
					<ul class="pagination pagination-sm">
						{#if currentPage > 1}
							<li class="page-item">
								<a class="page-link" href="{ROUTES.USER_FOLLOWERS(profile.username)}?page={currentPage - 1}">Previous</a>
							</li>
						{/if}
						{#each Array.from({ length: Math.min(totalPages, 10) }, (_, i) => i + 1) as p}
							<li class="page-item" class:active={p === currentPage}>
								<a class="page-link" href="{ROUTES.USER_FOLLOWERS(profile.username)}?page={p}">{p}</a>
							</li>
						{/each}
						{#if currentPage < totalPages}
							<li class="page-item">
								<a class="page-link" href="{ROUTES.USER_FOLLOWERS(profile.username)}?page={currentPage + 1}">Next</a>
							</li>
						{/if}
					</ul>
				</nav>
			{/if}
		{:else}
			<div class="fr-feed">
				<div class="fr-empty-state">
					<div class="fr-empty-icon"><i class="fa-solid fa-users"></i></div>
					<h2 class="fr-empty-title">No followers yet</h2>
					<p class="fr-empty-sub">This user doesn't have any followers yet.</p>
				</div>
			</div>
		{/if}
	</div>

	<aside class="fr-right-panel">
		<div class="fr-panel">
			<div class="fr-panel-header">Stats</div>
			<div class="fr-panel-stat">
				<span class="text-muted small">Total followers</span>
				<span class="fw-semibold small">{(data.total ?? 0).toLocaleString()}</span>
			</div>
			{#if totalPages > 1}
				<div class="fr-panel-stat">
					<span class="text-muted small">Page</span>
					<span class="fw-semibold small">{currentPage} / {totalPages}</span>
				</div>
			{/if}
		</div>

		<div class="fr-panel">
			<div class="fr-panel-header">{profile.display_name ?? profile.username}</div>
			<div class="fr-panel-body">
				<a href={ROUTES.USER_PROFILE(profile.username)} class="fr-panel-row">
					<i class="fa-solid fa-user rp-icon"></i>
					View profile
				</a>
				<a href={ROUTES.USER_FOLLOWING(profile.username)} class="fr-panel-row">
					<i class="fa-solid fa-user-group rp-icon"></i>
					Following
				</a>
			</div>
		</div>
	</aside>
</div>

<style>
	/* ── Profile card (mirrors u/[username]/+page.svelte) ── */
	.fr-profile-banner {
		height: 88px;
		background: linear-gradient(
			135deg,
			hsl(calc(var(--profile-hue) + 20), 55%, 38%) 0%,
			hsl(var(--profile-hue), 65%, 52%) 100%
		);
		border-top-left-radius: 7px;
		border-top-right-radius: 7px;
	}
	:global([data-bs-theme='dark']) .fr-profile-banner { filter: brightness(0.55) saturate(1.3); }
	.fr-profile-banner--photo { background-size: cover; background-position: center; }
	:global([data-bs-theme='dark']) .fr-profile-banner--photo { filter: brightness(0.75); }

	.fr-profile-avatar-abs {
		position: absolute;
		left: 1.25rem;
		top: calc(88px - 40px);
		width: 80px;
		height: 80px;
		border-radius: 50%;
		overflow: hidden;
		line-height: 0;
		z-index: 1;
		box-shadow: 0 0 0 3px var(--bs-body-bg), 0 2px 8px rgba(0, 0, 0, 0.25);
	}

	.fr-profile-body { padding-top: 52px; }

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
	.fr-profile-stat-item + .fr-profile-stat-item { border-left: 1px solid var(--bs-border-color); }

	.fr-stat-active { background: color-mix(in srgb, var(--bs-primary) 6%, transparent); }

	.fr-stat-label {
		font-size: 0.6875rem;
		color: var(--bs-secondary-color);
		text-transform: uppercase;
		letter-spacing: 0.05em;
		font-weight: 500;
	}
	.fr-stat-label-active { color: var(--bs-primary); }

	/* ── Follower rows ── */
	.fr-user-row {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.75rem 1rem;
		border-bottom: 1px solid var(--bs-border-color);
		text-decoration: none;
		color: inherit;
		transition: background 0.08s;
	}
	.fr-user-row:last-child { border-bottom: none; }
	.fr-user-row:hover { background: var(--bs-tertiary-bg); }

	.fr-user-body  { flex: 1; min-width: 0; }
	.fr-user-name  { font-size: 0.9375rem; color: var(--bs-body-color); }
	.fr-user-meta  { font-size: 0.75rem; color: var(--bs-tertiary-color); margin-top: 0.125rem; }
	.fr-user-sep   { opacity: 0.4; margin: 0 0.1875rem; }
	.fr-user-arrow { font-size: 0.6875rem; color: var(--bs-border-color); flex-shrink: 0; }

	.rp-icon { width: 1rem; text-align: center; font-size: 0.75rem; }
</style>
