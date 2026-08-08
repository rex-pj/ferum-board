import { api } from './api';

export async function getBookmarkStatus(threadSlug: string): Promise<boolean | null> {
  const res = await api.get(`/api/threads/${threadSlug}/bookmarks`);
  if (!res.ok) return null;
  const data = await res.json().catch(() => null) as { data?: { bookmarked?: boolean } } | null;
  return data?.data?.bookmarked ?? null;
}

export async function toggleBookmark(threadSlug: string, bookmarked: boolean): Promise<boolean> {
  const res = bookmarked
    ? await api.delete(`/api/threads/${threadSlug}/bookmarks`)
    : await api.post(`/api/threads/${threadSlug}/bookmarks`);
  return res.ok;
}
