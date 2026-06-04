<script lang="ts">
	interface Props {
		name?: string;
		currentUrl?: string | null;
		onFileChange?: (file: File | null) => void;
	}

	let { name = 'thumbnail', currentUrl = null, onFileChange }: Props = $props();

	let inputEl: HTMLInputElement;
	let preview = $state<string | null>(null);
	let selectedFile = $state<File | null>(null);
	let isDragOver = $state(false);
	let validationError = $state('');

	const ACCEPTED = ['image/jpeg', 'image/png', 'image/webp', 'image/gif'];
	const MAX_BYTES = 2 * 1024 * 1024;

	function handleFile(file: File) {
		validationError = '';
		if (!ACCEPTED.includes(file.type)) {
			validationError = 'Unsupported format. Please use JPEG, PNG, WebP or GIF.';
			return;
		}
		if (file.size > MAX_BYTES) {
			validationError = 'File exceeds the 2 MB limit.';
			return;
		}
		selectedFile = file;
		const reader = new FileReader();
		reader.onload = () => { preview = reader.result as string; };
		reader.readAsDataURL(file);
		onFileChange?.(file);
	}

	function onInputChange(e: Event) {
		const file = (e.target as HTMLInputElement).files?.[0];
		if (file) handleFile(file);
	}

	function onDrop(e: DragEvent) {
		e.preventDefault();
		isDragOver = false;
		const file = e.dataTransfer?.files?.[0];
		if (file) handleFile(file);
	}

	export function clear() {
		preview = null;
		selectedFile = null;
		validationError = '';
		if (inputEl) inputEl.value = '';
		onFileChange?.(null);
	}

	function formatBytes(n: number) {
		return n < 1024 * 1024 ? `${Math.round(n / 1024)} KB` : `${(n / (1024 * 1024)).toFixed(1)} MB`;
	}
</script>

<div class="tp">
	<!-- Current thumbnail (faded, shown when nothing new selected) -->
	{#if currentUrl && !preview}
		<div class="tp-img-wrap tp-img-wrap--current mb-2">
			<img src={currentUrl} alt="Current thumbnail" />
			<span class="tp-badge">Current</span>
		</div>
	{/if}

	<!-- New file preview -->
	{#if preview && selectedFile}
		<div class="tp-img-wrap mb-1">
			<img src={preview} alt="New thumbnail preview" />
			<button type="button" class="tp-clear" onclick={clear} title="Remove selection">
				<i class="fa-solid fa-xmark"></i>
			</button>
		</div>
		<p class="form-text mb-2">
			<i class="fa-solid fa-circle-check text-success me-1"></i>
			{selectedFile.name} &middot; {formatBytes(selectedFile.size)}
		</p>
	{/if}

	{#if validationError}
		<p class="text-danger small mb-2">
			<i class="fa-solid fa-circle-exclamation me-1"></i>{validationError}
		</p>
	{/if}

	<!-- Drop zone -->
	<!-- svelte-ignore a11y_interactive_supports_focus -->
	<div
		class="tp-zone {isDragOver ? 'tp-zone--over' : ''}"
		role="button"
		tabindex="0"
		aria-label="Upload thumbnail"
		onclick={() => inputEl?.click()}
		onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); inputEl?.click(); } }}
		ondragover={(e) => { e.preventDefault(); isDragOver = true; }}
		ondragleave={() => { isDragOver = false; }}
		ondrop={onDrop}
	>
		<i class="fa-solid fa-cloud-arrow-up tp-zone-icon"></i>
		<span class="tp-zone-label">
			{#if preview}
				Drop a new image or <u>click to change</u>
			{:else if currentUrl}
				Drop to replace or <u>click to browse</u>
			{:else}
				Drop an image here or <u>click to browse</u>
			{/if}
		</span>
		<span class="tp-zone-hint">JPEG, PNG, WebP or GIF &middot; max 2 MB</span>
	</div>

	<input
		bind:this={inputEl}
		type="file"
		{name}
		accept="image/jpeg,image/png,image/webp,image/gif"
		class="d-none"
		onchange={onInputChange}
	/>
</div>

<style>
	.tp {
		width: 100%;
		max-width: 320px;
	}

	/* ── Thumbnail preview ── */
	.tp-img-wrap {
		position: relative;
		border-radius: 0.5rem;
		overflow: hidden;
		line-height: 0;
	}

	.tp-img-wrap img {
		width: 100%;
		height: 160px;
		object-fit: cover;
		display: block;
	}

	.tp-img-wrap--current img {
		opacity: 0.6;
	}

	.tp-badge {
		position: absolute;
		bottom: 0.4rem;
		left: 0.4rem;
		background: rgba(0, 0, 0, 0.55);
		color: #fff;
		font-size: 0.7rem;
		padding: 0.1rem 0.45rem;
		border-radius: 0.25rem;
		line-height: 1.5;
	}

	.tp-clear {
		position: absolute;
		top: 0.4rem;
		right: 0.4rem;
		width: 28px;
		height: 28px;
		border: none;
		border-radius: 50%;
		background: rgba(220, 53, 69, 0.88);
		color: #fff;
		display: flex;
		align-items: center;
		justify-content: center;
		cursor: pointer;
		font-size: 0.75rem;
		padding: 0;
		line-height: 1;
		transition: background 0.1s;
	}

	.tp-clear:hover { background: #dc3545; }

	/* ── Drop zone ── */
	.tp-zone {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.3rem;
		border: 2px dashed var(--bs-border-color);
		border-radius: 0.5rem;
		padding: 1.5rem 1rem 1.25rem;
		text-align: center;
		cursor: pointer;
		transition: border-color 0.15s, background 0.15s, color 0.15s;
		color: var(--bs-secondary-color);
		user-select: none;
		-webkit-user-select: none;
	}

	.tp-zone:hover,
	.tp-zone--over {
		border-color: var(--bs-primary);
		color: var(--bs-primary);
	}

	.tp-zone--over {
		border-style: solid;
		background: rgba(var(--bs-primary-rgb), 0.07);
	}

	.tp-zone-icon {
		display: block;
		font-size: 1.75rem;
		opacity: 0.45;
		transition: opacity 0.15s, transform 0.15s;
	}

	.tp-zone:hover .tp-zone-icon,
	.tp-zone--over .tp-zone-icon {
		opacity: 0.75;
		transform: translateY(-2px);
	}

	.tp-zone-label {
		font-size: 0.8125rem;
	}

	.tp-zone-hint {
		font-size: 0.725rem;
		opacity: 0.65;
	}
</style>
