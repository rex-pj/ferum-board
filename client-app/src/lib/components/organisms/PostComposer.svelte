<script lang="ts">
	import { renderMarkdownPreview } from '$lib/utils/markdown';

	interface Props {
		value?: string;
		placeholder?: string;
		minlength?: number;
		name?: string;
		disabled?: boolean;
		class?: string;
	}

	let {
		value = $bindable(''),
		placeholder = 'Write your reply in Markdown…',
		minlength = 1,
		name = 'content_md',
		disabled = false,
		class: extraClass = ''
	}: Props = $props();

	let tab = $state<'write' | 'preview'>('write');
	let preview = $derived(tab === 'preview' ? renderMarkdownPreview(value) : '');
	let textarea: HTMLTextAreaElement;

	function wrap(before: string, after: string, placeholder: string) {
		if (!textarea) return;
		const start = textarea.selectionStart;
		const end = textarea.selectionEnd;
		const selected = value.slice(start, end) || placeholder;
		value = value.slice(0, start) + before + selected + after + value.slice(end);
		setTimeout(() => {
			textarea.focus();
			textarea.setSelectionRange(start + before.length, start + before.length + selected.length);
		}, 0);
	}

	function prefixLines(prefix: string) {
		if (!textarea) return;
		const start = textarea.selectionStart;
		const end = textarea.selectionEnd;
		const before = value.slice(0, start);
		const selected = value.slice(start, end) || 'list item';
		const after = value.slice(end);
		const prefixed = selected
			.split('\n')
			.map((l) => prefix + l)
			.join('\n');
		value = before + prefixed + after;
		setTimeout(() => {
			textarea.focus();
			textarea.setSelectionRange(start, start + prefixed.length);
		}, 0);
	}

	const tools = [
		{ icon: 'fa-bold',        title: 'Bold',         action: () => wrap('**', '**', 'bold text') },
		{ icon: 'fa-italic',      title: 'Italic',       action: () => wrap('*', '*', 'italic text') },
		{ icon: 'fa-code',        title: 'Inline code',  action: () => wrap('`', '`', 'code') },
		{ icon: 'fa-link',        title: 'Link',         action: () => wrap('[', '](url)', 'link text') },
		{ icon: 'fa-image',       title: 'Image',        action: () => wrap('![', '](url)', 'alt text') },
		{ icon: 'fa-quote-left',  title: 'Blockquote',   action: () => prefixLines('> ') },
		{ icon: 'fa-list-ul',     title: 'Unordered list', action: () => prefixLines('- ') },
		{ icon: 'fa-terminal',    title: 'Code block',   action: () => wrap('\n```\n', '\n```\n', 'code here') },
	] as const;
</script>

<div class="composer border rounded {extraClass}">
	<div class="d-flex align-items-center border-bottom px-2 pt-1 gap-1 flex-wrap">
		<button
			type="button"
			class="btn btn-link btn-sm px-2 pb-2 text-decoration-none border-bottom border-2 me-1 {tab === 'write' ? 'border-primary text-body' : 'border-transparent text-muted'}"
			onclick={() => (tab = 'write')}
		>
			Write
		</button>
		<button
			type="button"
			class="btn btn-link btn-sm px-2 pb-2 text-decoration-none border-bottom border-2 me-2 {tab === 'preview' ? 'border-primary text-body' : 'border-transparent text-muted'}"
			onclick={() => (tab = 'preview')}
			disabled={!value}
		>
			Preview
		</button>

		{#if tab === 'write'}
			<div class="vr mx-1 toolbar-sep"></div>
			{#each tools as tool}
				<button
					type="button"
					class="btn btn-sm btn-link text-muted text-decoration-none p-1 tool-btn"
					title={tool.title}
					aria-label={tool.title}
					{disabled}
					onclick={tool.action}
				>
					<i class="fa-solid {tool.icon} fa-sm"></i>
				</button>
			{/each}
		{/if}
	</div>

	{#if tab === 'write'}
		<textarea
			{name}
			bind:value
			bind:this={textarea}
			{placeholder}
			{minlength}
			{disabled}
			rows="8"
			class="form-control border-0 rounded-0 rounded-bottom font-monospace composer-textarea"
			required
		></textarea>
	{:else}
		<div class="p-3 prose preview-area">
			{#if preview}
				<!-- eslint-disable-next-line svelte/no-at-html-tags -->
				{@html preview}
			{:else}
				<span class="text-muted">Nothing to preview.</span>
			{/if}
		</div>
	{/if}
</div>

<div class="text-muted small mt-1">
	<i class="fa-brands fa-markdown me-1"></i>Markdown supported
</div>

<style>
	.toolbar-sep {
		height: 20px;
		opacity: 0.25;
	}

	.tool-btn {
		min-width: 30px;
		min-height: 30px;
		line-height: 1;
	}

	.composer-textarea {
		resize: vertical;
	}

	.preview-area {
		min-height: 200px;
	}
</style>
