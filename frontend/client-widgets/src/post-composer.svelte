<svelte:options customElement={{ tag: "ferum-post-composer", shadow: "none" }} />

<script lang="ts">
  import { previewMarkdown, uploadAttachment } from './lib/posts';
  import { t } from './lib/i18n';

  let {
    'thread-slug': threadSlug = '',
    'parent-id': parentId = '',
    'initial-content': initialContent = '',
    mode = 'reply',
    'can-upload': canUploadAttr = '',
  } = $props<{
    'thread-slug'?: string;
    'parent-id'?: string;
    'initial-content'?: string;
    mode?: string;
    'can-upload'?: string;
  }>();

  const canUpload = canUploadAttr === 'true';

  let content = $state(initialContent);
  let preview = $state('');
  let tab = $state<'write' | 'preview'>('write');
  let error = $state('');
  let textarea: HTMLTextAreaElement | null = $state(null);
  let fileInput: HTMLInputElement | null = $state(null);
  let uploading = $state(false);

  export function getContent() {
    return content;
  }

  export function reset() {
    content = '';
    preview = '';
    error = '';
    tab = 'write';
  }

  async function loadPreview() {
    if (!content.trim()) { preview = ''; return; }
    preview = await previewMarkdown(content);
  }

  function wrap(before: string, after: string, placeholder: string) {
    if (!textarea) return;
    const start = textarea.selectionStart;
    const end = textarea.selectionEnd;
    const selected = content.slice(start, end) || placeholder;
    content = content.slice(0, start) + before + selected + after + content.slice(end);
    // restore selection after tick
    setTimeout(() => {
      if (!textarea) return;
      const newStart = start + before.length;
      const newEnd = newStart + selected.length;
      textarea.focus();
      textarea.setSelectionRange(newStart, newEnd);
    }, 0);
  }

  function insertLine(prefix: string, placeholder: string) {
    if (!textarea) return;
    const start = textarea.selectionStart;
    const lineStart = content.lastIndexOf('\n', start - 1) + 1;
    const lineEnd = content.indexOf('\n', start);
    const end = lineEnd === -1 ? content.length : lineEnd;
    const line = content.slice(lineStart, end);
    const trimmed = line.trimStart();
    // If the line already starts with the prefix, remove it; else add it.
    if (trimmed.startsWith(prefix)) {
      content = content.slice(0, lineStart) + trimmed.slice(prefix.length) + content.slice(end);
    } else {
      if (trimmed) {
        content = content.slice(0, lineStart) + prefix + trimmed + content.slice(end);
      } else {
        content = content.slice(0, lineStart) + prefix + placeholder + content.slice(end);
      }
    }
    setTimeout(() => { textarea?.focus(); }, 0);
  }

  function insertLink() {
    if (!textarea) return;
    const start = textarea.selectionStart;
    const end = textarea.selectionEnd;
    const selected = content.slice(start, end);
    const text = selected || 'link text';
    const inserted = `[${text}](url)`;
    content = content.slice(0, start) + inserted + content.slice(end);
    setTimeout(() => {
      if (!textarea) return;
      // Select the url part
      const urlStart = start + text.length + 3;
      textarea.focus();
      textarea.setSelectionRange(urlStart, urlStart + 3);
    }, 0);
  }

  function insertCodeBlock() {
    if (!textarea) return;
    const start = textarea.selectionStart;
    const end = textarea.selectionEnd;
    const selected = content.slice(start, end);
    const nl = content[start - 1] === '\n' || start === 0 ? '' : '\n';
    const inserted = `${nl}\`\`\`\n${selected || 'code here'}\n\`\`\`\n`;
    content = content.slice(0, start) + inserted + content.slice(end);
    setTimeout(() => {
      if (!textarea) return;
      const codeStart = start + nl.length + 4;
      const codeEnd = codeStart + (selected || 'code here').length;
      textarea.focus();
      textarea.setSelectionRange(codeStart, codeEnd);
    }, 0);
  }

  function triggerAttach() {
    fileInput?.click();
  }

  async function onFileSelected(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = ''; // allow re-selecting the same file later
    if (!file) return;

    uploading = true;
    error = '';
    const result = await uploadAttachment(file);
    uploading = false;

    if (!result.ok || !result.url) {
      error = result.error || t('js-failed-upload-image');
      return;
    }

    const insertPos = textarea ? textarea.selectionStart : content.length;
    const markdown = `![${file.name}](${result.url})`;
    const nl = insertPos > 0 && content[insertPos - 1] !== '\n' ? '\n' : '';
    content = content.slice(0, insertPos) + nl + markdown + '\n' + content.slice(insertPos);
    setTimeout(() => { textarea?.focus(); }, 0);
  }

  const tools: Array<{ icon: string; title: string; action: () => void } | 'sep'> = [
    { icon: 'fa-solid fa-bold',          title: t('js-composer-bold'),  action: () => wrap('**', '**', 'bold text') },
    { icon: 'fa-solid fa-italic',        title: t('js-composer-italic'), action: () => wrap('_', '_', 'italic text') },
    { icon: 'fa-solid fa-strikethrough', title: t('js-composer-strikethrough'),  action: () => wrap('~~', '~~', 'strikethrough') },
    'sep',
    { icon: 'H1', title: t('js-composer-heading-1'),   action: () => insertLine('# ', 'Heading 1') },
    { icon: 'H2', title: t('js-composer-heading-2'),   action: () => insertLine('## ', 'Heading 2') },
    'sep',
    { icon: 'fa-solid fa-quote-left',   title: t('js-composer-quote'),           action: () => insertLine('> ', 'quoted text') },
    { icon: 'fa-solid fa-code',         title: t('js-composer-code-inline'),   action: () => wrap('`', '`', 'code') },
    { icon: 'fa-solid fa-terminal',     title: t('js-composer-code-block'),      action: () => insertCodeBlock() },
    'sep',
    { icon: 'fa-solid fa-link',         title: t('js-composer-link'),   action: () => insertLink() },
    { icon: 'fa-solid fa-list-ul',      title: t('js-composer-ul'),  action: () => insertLine('- ', 'list item') },
    { icon: 'fa-solid fa-list-ol',      title: t('js-composer-ol'),    action: () => insertLine('1. ', 'list item') },
  ];

  function onKeydown(e: KeyboardEvent) {
    if (e.ctrlKey || e.metaKey) {
      if (e.key === 'b') { e.preventDefault(); wrap('**', '**', 'bold text'); }
      if (e.key === 'i') { e.preventDefault(); wrap('_', '_', 'italic text'); }
      if (e.key === 'k') { e.preventDefault(); insertLink(); }
    }
  }
</script>

<div class="composer">
  <!-- Tab bar + toolbar row -->
  <div class="composer-header">
    <div class="tab-bar">
      <button type="button" class="tab {tab === 'write' ? 'active' : ''}" onclick={() => (tab = 'write')}>{t('js-composer-write')}</button>
      <button type="button" class="tab {tab === 'preview' ? 'active' : ''}" onclick={() => { tab = 'preview'; loadPreview(); }}>{t('js-composer-preview')}</button>
    </div>

    {#if tab === 'write'}
      <div class="toolbar" role="toolbar" aria-label={t('js-composer-toolbar')}>
        {#each tools as tool}
          {#if tool === 'sep'}
            <span class="sep" role="separator"></span>
          {:else}
            <button
              type="button"
              class="tool-btn"
              title={tool.title}
              onclick={tool.action}
              tabindex="-1"
            >{#if tool.icon.startsWith('fa-')}<i class={tool.icon} aria-hidden="true"></i>{:else}{tool.icon}{/if}</button>
          {/if}
        {/each}
        {#if canUpload}
          <span class="sep" role="separator"></span>
          <button
            type="button"
            class="tool-btn"
            title={t('js-composer-attach-image')}
            onclick={triggerAttach}
            disabled={uploading}
            tabindex="-1"
          ><i class={uploading ? 'fa-solid fa-spinner fa-spin' : 'fa-regular fa-image'} aria-hidden="true"></i></button>
          <input
            bind:this={fileInput}
            type="file"
            accept="image/jpeg,image/png,image/webp,image/gif"
            class="visually-hidden-input"
            onchange={onFileSelected}
          >
        {/if}
      </div>
    {/if}
  </div>

  {#if tab === 'write'}
    <textarea
      bind:this={textarea}
      class="editor"
      placeholder={t('js-composer-placeholder')}
      bind:value={content}
      rows={8}
      onkeydown={onKeydown}
    ></textarea>
  {:else}
    <div class="preview post-content">
      {#if preview}
        <!-- eslint-disable-next-line svelte/no-at-html-tags -->
        {@html preview}
      {:else}
        <span class="empty">{t('js-composer-nothing-to-preview')}</span>
      {/if}
    </div>
  {/if}

  {#if error}
    <div class="error">{error}</div>
  {/if}

  <div class="actions">
    <span class="hint"><i class="fa-brands fa-markdown"></i> {t('js-composer-hint')}</span>
  </div>
</div>

<style>
  /* CSS custom properties pierce shadow DOM via inheritance — picks up
     Bootstrap / theme tokens set on :root or [data-bs-theme].          */
  .composer {
    display: flex;
    flex-direction: column;
    gap: 0;
    border: 1px solid var(--bs-border-color, #dee2e6);
    border-radius: 8px;
    overflow: hidden;
    background: var(--bs-body-bg, #fff);
    color: var(--bs-body-color, #212529);
  }

  /* Header = tabs + toolbar side-by-side */
  .composer-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    background: var(--bs-tertiary-bg, #f8f9fa);
    border-bottom: 1px solid var(--bs-border-color, #dee2e6);
    padding: 0 8px 0 0;
    flex-wrap: wrap;
    gap: 4px;
  }

  .tab-bar {
    display: flex;
  }

  .tab {
    padding: 8px 16px;
    border: none;
    background: transparent;
    cursor: pointer;
    font-size: 0.875rem;
    border-bottom: 2px solid transparent;
    min-height: 44px;
    color: var(--bs-secondary-color, #6c757d);
    transition: color 0.15s, border-color 0.15s;
  }
  .tab.active {
    border-bottom-color: var(--ferum-primary, #6366f1);
    color: var(--ferum-primary, #6366f1);
    font-weight: 600;
  }
  .tab:hover:not(.active) { color: var(--bs-body-color, #212529); }

  /* Toolbar */
  .toolbar {
    display: flex;
    align-items: center;
    gap: 1px;
    flex-wrap: wrap;
    padding: 4px 0;
  }

  .tool-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 44px;
    height: 44px;
    padding: 0 6px;
    border: none;
    background: transparent;
    cursor: pointer;
    font-size: 0.8rem;
    font-weight: 600;
    color: var(--bs-secondary-color, #6c757d);
    border-radius: 4px;
    line-height: 1;
    transition: background 0.1s, color 0.1s;
    white-space: nowrap;
  }
  .tool-btn:hover {
    background: var(--bs-secondary-bg, #e9ecef);
    color: var(--bs-body-color, #212529);
  }
  .tool-btn:active {
    background: var(--bs-border-color, #dee2e6);
  }

  .visually-hidden-input {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    border: 0;
  }

  .sep {
    display: inline-block;
    width: 1px;
    height: 20px;
    background: var(--bs-border-color, #dee2e6);
    margin: 0 4px;
    align-self: center;
  }

  /* Editor area */
  .editor {
    width: 100%;
    padding: 12px 14px;
    border: none;
    outline: none;
    font-family: 'SFMono-Regular', Consolas, 'Liberation Mono', Menlo, monospace;
    font-size: 0.875rem;
    line-height: 1.6;
    resize: vertical;
    background: var(--bs-body-bg, #fff);
    color: var(--bs-body-color, #212529);
    min-height: 180px;
  }

  .preview {
    min-height: 180px;
    padding: 12px 14px;
    font-size: 0.9rem;
    line-height: 1.7;
    background: var(--bs-body-bg, #fff);
    color: var(--bs-body-color, #212529);
  }
  .empty { color: var(--bs-secondary-color, #6c757d); font-style: italic; }

  .error { color: #dc3545; font-size: 0.85rem; padding: 6px 14px; }

  .actions {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 8px 12px;
    border-top: 1px solid var(--bs-border-color, #dee2e6);
    background: var(--bs-tertiary-bg, #f8f9fa);
    flex-wrap: wrap;
    gap: 8px;
  }
  .hint { color: var(--bs-secondary-color, #6c757d); font-size: 0.75rem; }

  .submit-btn {
    padding: 6px 20px;
    background: var(--ferum-primary, #6366f1);
    color: #fff;
    border: none;
    border-radius: 6px;
    cursor: pointer;
    font-size: 0.875rem;
    min-height: 44px;
    font-weight: 500;
    transition: background 0.15s;
  }
  .submit-btn:hover:not(:disabled) { background: var(--ferum-primary-dark, #4f46e5); }
  .submit-btn:disabled { opacity: 0.6; cursor: not-allowed; }
</style>
