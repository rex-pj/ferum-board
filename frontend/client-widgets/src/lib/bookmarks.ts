import { api } from './api';

export async function getBookmarkStatus(threadId: string): Promise<boolean | null> {
  const res = await api.get(`/api/threads/${threadId}/bookmarks`);
  if (!res.ok) return null;
  const data = await res.json().catch(() => null) as { data?: { bookmarked?: boolean } } | null;
  return data?.data?.bookmarked ?? null;
}

export async function toggleBookmark(threadId: string, bookmarked: boolean): Promise<boolean> {
  const res = bookmarked
    ? await api.delete(`/api/threads/${threadId}/bookmarks`)
    : await api.post(`/api/threads/${threadId}/bookmarks`);
  return res.ok;
}
