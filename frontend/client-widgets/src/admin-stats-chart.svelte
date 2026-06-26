<svelte:options customElement={{ tag: "admin-stats-chart", shadow: "none" }} />

<script lang="ts">
  import { onMount } from 'svelte';

  let {
    dau = '0',
    mau = '0',
    'new-threads': newThreads = '0',
    'new-users': newUsers = '0',
    'new-posts': newPosts = '0',
    'new-reactions': newReactions = '0',
    'new-views': newViews = '0',
    'activation-rate-pct': activationRatePct = '0',
    'dau-mau-ratio': dauMauRatio = '0',
    days: initDays = '30',
  } = $props<{
    dau?: string;
    mau?: string;
    'new-threads'?: string;
    'new-users'?: string;
    'new-posts'?: string;
    'new-reactions'?: string;
    'new-views'?: string;
    'activation-rate-pct'?: string;
    'dau-mau-ratio'?: string;
    days?: string;
  }>();

  type Point = { date: string; value: number };

  const METRICS = [
    {
      key: 'dau',
      label: 'Active Users / Day',
      desc: 'Unique users who visited the forum today (Daily Active Users).',
      color: '#6366f1',
    },
    {
      key: 'new_users',
      label: 'New Registrations',
      desc: 'Number of new accounts created today.',
      color: '#f59e0b',
    },
    {
      key: 'new_threads',
      label: 'New Threads',
      desc: 'Threads created today (excluding deleted).',
      color: '#22c55e',
    },
    {
      key: 'new_posts',
      label: 'New Posts',
      desc: 'Posts published today (excluding deleted).',
      color: '#0ea5e9',
    },
    {
      key: 'new_reactions',
      label: 'New Reactions',
      desc: 'Reactions (like, helpful, insightful, funny) added today.',
      color: '#f43f5e',
    },
    {
      key: 'new_views',
      label: 'New Views',
      desc: 'Thread view count delta since yesterday\'s snapshot.',
      color: '#14b8a6',
    },
    {
      key: 'activation_rate_pct',
      label: 'Activation Rate',
      desc: '% of users who registered yesterday and posted at least once today.',
      color: '#a855f7',
      suffix: '%',
    },
  ] as const;

  type MetricKey = typeof METRICS[number]['key'];

  const RANGES = [7, 30, 90] as const;

  let activeMetric = $state<MetricKey>('dau');
  let activeDays   = $state(Number(initDays) || 30);
  let history      = $state<Point[]>([]);
  let loading      = $state(false);
  let error        = $state('');
  let ttVisible    = $state(false);
  let ttIdx        = $state(0);

  const snapshots = [
    { label: 'DAU',          value: Number(dau),               color: '#6366f1', title: 'Daily Active Users — unique visitors today' },
    { label: 'MAU',          value: Number(mau),               color: '#0ea5e9', title: 'Monthly Active Users — unique visitors in 30 days' },
    { label: 'New Threads',  value: Number(newThreads),        color: '#22c55e', title: 'Threads created today' },
    { label: 'New Users',    value: Number(newUsers),          color: '#f59e0b', title: 'New registrations today' },
    { label: 'New Posts',    value: Number(newPosts),          color: '#64748b', title: 'Posts published today' },
    { label: 'New Reactions',value: Number(newReactions),      color: '#f43f5e', title: 'Reactions added today' },
    { label: 'New Views',    value: Number(newViews),          color: '#14b8a6', title: 'Thread view count delta since yesterday' },
    { label: 'Activation',   value: Number(activationRatePct), color: '#a855f7', title: '% of yesterday\'s signups who posted today', suffix: '%' },
    { label: 'Stickiness',   value: Number(dauMauRatio),       color: '#ec4899', title: 'DAU / MAU × 100 — engagement retention ratio', suffix: '%' },
  ];

  async function loadHistory(metric: MetricKey, days: number) {
    loading = true;
    error = '';
    ttVisible = false;
    try {
      const res = await fetch(`/api/admin/stats/history?metric=${metric}&days=${days}`);
      if (!res.ok) throw new Error(await res.text());
      const json = await res.json();
      history = (json.data as Point[]).map((p) => ({ ...p, value: Number(p.value) }));
    } catch (e: any) {
      error = e.message ?? 'Failed to load data';
    } finally {
      loading = false;
    }
  }

  onMount(() => loadHistory(activeMetric, activeDays));

  function selectMetric(key: MetricKey) {
    activeMetric = key;
    loadHistory(key, activeDays);
  }

  function selectRange(d: number) {
    activeDays = d;
    loadHistory(activeMetric, d);
  }

  // ── SVG chart constants ────────────────────────────────────────────────────
  const W = 600, H = 160;
  const PAD = { top: 12, right: 12, bottom: 32, left: 44 };
  const innerW = W - PAD.left - PAD.right;
  const innerH = H - PAD.top - PAD.bottom;

  function xScale(i: number, len: number) {
    return PAD.left + (len < 2 ? 0 : (i / (len - 1)) * innerW);
  }
  function yScale(v: number, maxV: number) {
    return PAD.top + innerH - (v / maxV) * innerH;
  }

  // Raw max, then add 15% headroom so peak labels always sit inside the chart area.
  // Minimum 4 so the Y axis always has 5 evenly-spaced ticks even when all data is zero.
  const chartMaxV = $derived(
    history.length > 0
      ? Math.max(4, Math.ceil(Math.max(...history.map((p) => p.value)) * 1.15))
      : 4
  );

  // Label-worthy indices — three cases:
  //   1. True local max:    v > prev  AND  v > next         (single spike)
  //   2. Plateau entry:     v > prev  AND  v === next        (rising into flat run)
  //   3. Plateau exit:      v === prev AND  v > next         (falling out of flat run)
  // Cases 1-3 collapse to: (v > prev && v >= next) || (v >= prev && v > next)
  // In practice: label a point when it is strictly greater than AT LEAST ONE neighbour
  // AND at least as large as the other — i.e. NOT in the interior of a plateau.
  //   4. Last non-zero point (today) — always shown regardless of neighbours.
  // Deduplicated by minimum x-gap (40 SVG units ≈ 4-digit label at font 9).
  // When two labels would collide, the later (closer-to-today) one wins.
  const peakIndices = $derived((() => {
    const n = history.length;
    if (n === 0) return [] as number[];
    const MIN_GAP = 40;

    const candidates: number[] = [];
    for (let i = 0; i < n; i++) {
      const v = history[i].value;
      if (v <= 0) continue;
      const prev = i > 0     ? history[i - 1].value : -Infinity;
      const next = i < n - 1 ? history[i + 1].value : -Infinity;
      // Inflection: local max OR plateau entry OR plateau exit
      if ((v > prev && v >= next) || (v >= prev && v > next)) {
        candidates.push(i);
      }
    }
    // Always show the last non-zero point (today).
    const last = n - 1;
    if (history[last].value > 0 && !candidates.includes(last)) {
      candidates.push(last);
    }

    // Drop any candidate that has a LATER candidate within MIN_GAP (later wins).
    return candidates.filter((ci) =>
      !candidates.some(
        (cj) => cj > ci && Math.abs(xScale(cj, n) - xScale(ci, n)) < MIN_GAP
      )
    );
  })());

  function chartPath(maxV: number) {
    if (history.length < 2) return '';
    return history.map((p, i) =>
      `${i === 0 ? 'M' : 'L'}${xScale(i, history.length).toFixed(1)},${yScale(p.value, maxV).toFixed(1)}`
    ).join(' ');
  }

  function xLabelItems() {
    if (history.length === 0) return [];
    const step = Math.max(1, Math.ceil(history.length / 7));
    return history
      .map((p, i) => ({ i, label: p.date.slice(5) }))
      .filter((_, i) => i % step === 0 || i === history.length - 1);
  }

  function yTickItems(maxV: number) {
    const fracs = [1, 0.75, 0.5, 0.25, 0];
    const seen = new Set<number>();
    return fracs
      .map((f) => Math.round(f * maxV))
      .filter((v) => { const ok = !seen.has(v); seen.add(v); return ok; })
      .map((v) => ({ y: yScale(v, maxV), label: v.toLocaleString() }));
  }

  const activeMeta   = $derived(METRICS.find((m) => m.key === activeMetric) ?? METRICS[0]);
  const activeColor  = $derived(activeMeta.color);
  const activeSuffix = $derived('suffix' in activeMeta ? (activeMeta as any).suffix ?? '' : '');
  const activeDesc   = $derived(activeMeta.desc);

  // Tooltip X position as % of chart-wrap width
  const ttLeftPct = $derived(
    ttVisible && history.length >= 2
      ? (xScale(ttIdx, history.length) / W) * 100
      : 0
  );

  // ── Mouse handlers ─────────────────────────────────────────────────────────
  function hitIdx(e: MouseEvent) {
    if (history.length < 2) return -1;
    const wrap = e.currentTarget as HTMLElement;
    const rect = wrap.getBoundingClientRect();
    const chartStartPx = (PAD.left / W) * rect.width;
    const chartEndPx   = ((PAD.left + innerW) / W) * rect.width;
    const clamped = Math.max(chartStartPx, Math.min(chartEndPx, e.clientX - rect.left));
    const frac = (clamped - chartStartPx) / (chartEndPx - chartStartPx);
    return Math.max(0, Math.min(history.length - 1, Math.round(frac * (history.length - 1))));
  }

  function onChartMouseMove(e: MouseEvent) {
    if (loading) return;
    const idx = hitIdx(e);
    if (idx < 0) return;
    ttVisible = true;
    ttIdx = idx;
  }

  function onChartMouseLeave() {
    ttVisible = false;
  }

  // P3: drill-down — navigate to most relevant admin page on click
  const DRILL_DOWN: Record<MetricKey, string> = {
    dau:                 '/admin/users',
    new_users:           '/admin/users',
    new_threads:         '/forum',
    new_posts:           '/forum',
    new_reactions:       '/forum',
    new_views:           '/forum',
    activation_rate_pct: '/admin/users',
  };

  function onChartClick(e: MouseEvent) {
    if (loading || history.length < 2) return;
    window.location.href = DRILL_DOWN[activeMetric] ?? '/admin';
  }
</script>

<!-- Snapshot cards row -->
<div class="snapshot-row">
  {#each snapshots as s}
    <div class="snapshot-card" title={s.title}>
      <div class="snapshot-value" style="color:{s.color}">{s.value.toLocaleString()}{s.suffix ?? ''}</div>
      <div class="snapshot-label">{s.label}</div>
    </div>
  {/each}
</div>

<!-- Controls: metric tabs + date range selector -->
<div class="controls-row">
  <div class="metric-tabs">
    {#each METRICS as m}
      <button
        class="metric-tab"
        class:active={activeMetric === m.key}
        style={activeMetric === m.key ? `border-color:${m.color};color:${m.color}` : ''}
        onclick={() => selectMetric(m.key)}
        title={m.desc}
      >{m.label}</button>
    {/each}
  </div>
  <div class="range-tabs">
    {#each RANGES as d}
      <button
        class="range-tab"
        class:active={activeDays === d}
        onclick={() => selectRange(d)}
      >{d}d</button>
    {/each}
  </div>
</div>

<!-- Metric description -->
<p class="metric-desc">{activeDesc}</p>

<!-- Chart area -->
<div
  class="chart-wrap"
  class:chart-clickable={!loading && history.length >= 2}
  onmousemove={onChartMouseMove}
  onmouseleave={onChartMouseLeave}
  onclick={onChartClick}
  role="img"
  aria-label="Activity trend chart"
>
  {#if loading}
    <div class="chart-overlay">Loading…</div>
  {:else if error}
    <div class="chart-overlay text-danger">{error}</div>
  {:else if history.length > 0}
    <!-- preserveAspectRatio="none" is correct here: the container already has
         aspect-ratio:600/160 so the SVG fills it exactly without distortion or whitespace. -->
    <svg viewBox="0 0 {W} {H}" preserveAspectRatio="none" overflow="visible" class="line-chart">
      <!-- Y grid lines + labels -->
      {#each yTickItems(chartMaxV) as t}
        <line x1={PAD.left} y1={t.y} x2={W - PAD.right} y2={t.y} stroke="#e2e8f0" stroke-width="1" />
        <text x={PAD.left - 6} y={t.y + 4} text-anchor="end" class="axis-label">{t.label}</text>
      {/each}
      <!-- X labels -->
      {#each xLabelItems() as x}
        <text x={xScale(x.i, history.length)} y={H - 8} text-anchor="middle" class="axis-label">{x.label}</text>
      {/each}
      <!-- Area fill -->
      <path
        d="{chartPath(chartMaxV)} L{xScale(history.length - 1, history.length)},{PAD.top + innerH} L{PAD.left},{PAD.top + innerH} Z"
        fill={activeColor}
        fill-opacity="0.08"
      />
      <!-- Line -->
      <path d={chartPath(chartMaxV)} fill="none" stroke={activeColor} stroke-width="2" stroke-linejoin="round" />
      <!-- Data point dots -->
      {#each history as pt, i}
        <circle
          cx={xScale(i, history.length)}
          cy={yScale(pt.value, chartMaxV)}
          r={ttVisible && ttIdx === i ? 5 : 2.5}
          fill={activeColor}
          fill-opacity={ttVisible && ttIdx === i ? 1 : 0.5}
        />
      {/each}
      <!-- Peak labels: inflection points (local max + plateau entry/exit) + today.
           Always text-anchor="middle" so the label is centered on its dot.
           15 % headroom in chartMaxV guarantees the label never touches the top border. -->
      {#each peakIndices as pi}
        {@const pt  = history[pi]}
        {@const px  = xScale(pi, history.length)}
        {@const py  = yScale(pt.value, chartMaxV)}
        <circle cx={px} cy={py} r={4} fill={activeColor} />
        <text
          x={px}
          y={py - 10}
          text-anchor="middle"
          class="peak-label"
          fill={activeColor}
        >{pt.value.toLocaleString()}{activeSuffix}</text>
      {/each}
      <!-- Vertical crosshair at hover position -->
      {#if ttVisible && history[ttIdx]}
        <line
          x1={xScale(ttIdx, history.length)}
          y1={PAD.top}
          x2={xScale(ttIdx, history.length)}
          y2={PAD.top + innerH}
          stroke={activeColor}
          stroke-width="1"
          stroke-dasharray="3 3"
          opacity="0.6"
        />
      {/if}
    </svg>

    <!-- P2: Tooltip overlay -->
    {#if ttVisible && history[ttIdx]}
      <div
        class="chart-tooltip"
        style="left: clamp(8px, calc({ttLeftPct}% - 48px), calc(100% - 104px))"
      >
        <div class="tt-date">{history[ttIdx].date}</div>
        <div class="tt-value" style="color:{activeColor}">
          {history[ttIdx].value.toLocaleString()}{activeSuffix}
        </div>
      </div>
    {/if}

  {:else}
    <div class="chart-overlay text-muted">No data available for this period</div>
  {/if}
</div>
<!-- Drill-down hint sits below the chart, never inside it -->
{#if !loading && history.length >= 2}
  <div class="chart-hint">Click chart to view details →</div>
{/if}

<style>
  /* Host must be block so snapshot-row is constrained to card width */
  :global(admin-stats-chart) { display: block; min-width: 0; }

  /* Snapshot cards */
  .snapshot-row {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(80px, 1fr));
    gap: 10px;
    margin-bottom: 1rem;
  }
  .snapshot-card {
    background: var(--bs-secondary-bg, #f8f9fa);
    border-radius: 8px; padding: 8px 12px; text-align: center;
    min-width: 0;
  }
  .snapshot-value { font-size: 1.3rem; font-weight: 700; line-height: 1.2; }
  .snapshot-label { font-size: 0.68rem; color: #6c757d; margin-top: 2px; }

  /* Controls */
  .controls-row {
    display: flex; align-items: center; justify-content: space-between;
    flex-wrap: wrap; gap: 8px; margin-bottom: 0.75rem;
  }
  .metric-tabs { display: flex; gap: 6px; flex-wrap: wrap; }
  .metric-tab {
    padding: 3px 11px; border-radius: 20px; border: 2px solid #dee2e6;
    background: none; cursor: pointer; font-size: 0.78rem; color: #6c757d;
    transition: all .15s;
  }
  .metric-tab.active { font-weight: 600; }
  .metric-tab:hover:not(.active) { border-color: #adb5bd; }

  /* Date range tabs */
  .range-tabs { display: flex; gap: 4px; }
  .range-tab {
    padding: 3px 10px; border-radius: 6px; border: 1px solid #dee2e6;
    background: none; cursor: pointer; font-size: 0.75rem; color: #6c757d;
    transition: all .15s;
  }
  .range-tab.active { background: #6366f1; color: #fff; border-color: #6366f1; font-weight: 600; }
  .range-tab:hover:not(.active) { background: #f1f5f9; }

  /* Metric description */
  .metric-desc {
    font-size: 0.75rem; color: #64748b; margin: 0 0 0.5rem;
    min-height: 1.2em;
  }

  /* Chart */
  .chart-wrap {
    position: relative;
    aspect-ratio: 600 / 160;
    width: 100%;
    user-select: none;
  }
  .chart-wrap.chart-clickable { cursor: pointer; }
  .chart-overlay {
    position: absolute; inset: 0; display: flex;
    align-items: center; justify-content: center; font-size: 0.85rem;
  }
  .line-chart { width: 100%; height: 100%; display: block; }
  .axis-label { font-size: 9px; fill: #94a3b8; font-family: inherit; }
  .peak-label  { font-size: 9px; font-weight: 600; font-family: inherit; }

  /* P2: Tooltip */
  .chart-tooltip {
    position: absolute; top: 4px; width: 96px;
    background: rgba(15, 23, 42, 0.85); color: #fff;
    border-radius: 6px; padding: 5px 8px;
    pointer-events: none; font-size: 0.75rem; line-height: 1.4;
    backdrop-filter: blur(4px);
  }
  .tt-date  { color: #94a3b8; font-size: 0.68rem; }
  .tt-value { font-weight: 700; font-size: 0.88rem; }

  /* P3: Hint — lives below the chart, not overlapping x-labels */
  .chart-hint {
    text-align: right; font-size: 0.65rem; color: #94a3b8;
    margin-top: 2px; cursor: pointer;
  }
</style>
