<script lang="ts">
	import '../../app.css';

	let { children }: { children: any } = $props();
</script>

<div class="min-vh-100 d-flex align-items-center justify-content-center bg-body-secondary py-5">
	<div class="w-100 setup-wrapper">
		<div class="text-center mb-4">
			<h1 class="h3 fw-bold">Ferum Board</h1>
			<p class="text-muted mb-0">Installation Wizard</p>
		</div>
		<div class="card shadow-sm">
			<div class="card-body p-4 p-md-5">
				{@render children()}
			</div>
		</div>
	</div>
</div>

<style>
	.setup-wrapper { max-width: 640px; }
</style>
