import { api } from './api';

export type ReactionResult =
  | { ok: true }
  | { ok: false; code: string; message: string };

export async function toggleReaction(
  postId: string,
  kind: string,
  reacted: boolean,
): Promise<ReactionResult> {
  const res = reacted
    ? await api.delete(`/api/posts/${postId}/reactions/${kind}`)
    : await api.post(`/api/posts/${postId}/reactions`, { kind });

  if (res.ok) return { ok: true };

  try {
    const body = await res.json();
    const err = body?.error ?? {};
    return { ok: false, code: err.code ?? 'unknown', message: err.message ?? 'Failed to update reaction' };
  } catch {
    return { ok: false, code: 'unknown', message: 'Failed to update reaction' };
  }
}
