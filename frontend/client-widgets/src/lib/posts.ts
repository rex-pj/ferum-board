import { api } from './api';
import { shrinkForUpload } from './downscale';
import { t } from './i18n';

export async function previewMarkdown(content: string): Promise<string> {
  const res = await api.post('/api/preview-markdown', { content });
  if (!res.ok) return '';
  const data = await res.json().catch(() => ({})) as { html?: string };
  return data.html ?? '';
}

export async function uploadAttachment(file: File): Promise<{ ok: boolean; url?: string; error?: string }> {
  // Best-effort and never fatal — see `shrinkForUpload`. A phone photo goes out
  // at roughly a quarter the bytes; anything it cannot handle is uploaded as
  // picked, which is what happened before this call existed.
  const payload = await shrinkForUpload(file);
  const formData = new FormData();
  formData.append('file', payload);
  const res = await api.postForm('/api/posts/attachments', formData);
  if (res.ok) {
    const data = await res.json().catch(() => ({})) as { data?: { url?: string } };
    return { ok: true, url: data?.data?.url };
  }
  const body = await res.json().catch(() => ({})) as { error?: { message?: string } };
  return { ok: false, error: body?.error?.message ?? t('js-failed-upload-image') };
}

export async function createPost(
  threadSlug: string,
  contentMd: string,
  parentId?: string | null,
): Promise<{ ok: boolean; error?: string }> {
  const res = await api.post(`/api/threads/${threadSlug}/posts`, {
    content_md: contentMd,
    parent_id: parentId ?? null,
  });
  if (res.ok) return { ok: true };
  const body = await res.json().catch(() => ({})) as { error?: { message?: string } };
  return { ok: false, error: body?.error?.message ?? 'Failed to post reply.' };
}
