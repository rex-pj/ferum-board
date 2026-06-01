<script lang="ts">
	import type { ToastMessage } from '$lib/stores/toast';
	import { toast } from '$lib/stores/toast';

	let { item }: { item: ToastMessage } = $props();

	const CONFIG = {
		success: { icon: 'fa-circle-check',        color: 'text-success', label: 'Success' },
		error:   { icon: 'fa-circle-xmark',         color: 'text-danger',  label: 'Error'   },
		warning: { icon: 'fa-triangle-exclamation', color: 'text-warning', label: 'Warning' },
		info:    { icon: 'fa-circle-info',           color: 'text-info',    label: 'Info'    },
	} as const;

	const cfg = $derived(CONFIG[item.kind]);
</script>

<div
	class="toast show mb-2"
	role="alert"
	aria-live={item.kind === 'error' ? 'assertive' : 'polite'}
	aria-atomic="true"
>
	<div class="toast-header">
		<i class="fa-solid {cfg.icon} {cfg.color} me-2" aria-hidden="true"></i>
		<strong class="me-auto">{cfg.label}</strong>
		<button
			type="button"
			class="btn-close"
			aria-label="Dismiss"
			onclick={() => toast.dismiss(item.id)}
		></button>
	</div>
	<div class="toast-body">{item.message}</div>
</div>

<style>
	.toast {
		min-width: 280px;
		max-width: 380px;
		animation: toast-in 0.2s ease;
	}

	@keyframes toast-in {
		from { opacity: 0; transform: translateY(8px); }
		to   { opacity: 1; transform: translateY(0);   }
	}
</style>
