import { api } from './api';

export async function previewMarkdown(content: string): Promise<string> {
  const res = await api.post('/api/preview-markdown', { content });
  if (!res.ok) return '';
  const data = await res.json().catch(() => ({})) as { html?: string };
  return data.html ?? '';
}

export async function createPost(
  threadId: string,
  contentMd: string,
  parentId?: string | null,
): Promise<{ ok: boolean; error?: string }> {
  const res = await api.post(`/api/threads/${threadId}/posts`, {
    content_md: contentMd,
    parent_id: parentId ?? null,
  });
  if (res.ok) return { ok: true };
  const body = await res.json().catch(() => ({})) as { error?: { message?: string } };
  return { ok: false, error: body?.error?.message ?? 'Failed to post reply.' };
}
