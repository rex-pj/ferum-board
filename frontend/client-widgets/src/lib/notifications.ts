import { api } from './api';

export async function getUnreadCount(): Promise<number> {
  const res = await api.get('/api/notifications/unread-count');
  if (!res.ok) return 0;
  const data = await res.json().catch(() => ({})) as { data?: { count?: number } };
  return data.data?.count ?? 0;
}

export function openNotificationStream(handlers: {
  onNotification: () => void;
  onDisconnect: () => void;
}): () => void {
  const es = new EventSource('/api/notifications/stream');
  es.onopen = () => { /* connected */ };
  es.addEventListener('notification', handlers.onNotification);
  es.onerror = () => {
    es.close();
    handlers.onDisconnect();
  };

  // Close the stream when navigating away so the browser releases the connection
  // slot before the new page opens its own stream. Without this, each full-page
  // navigation (SSR) accumulates an open SSE connection; HTTP/1.1 caps ~6 per
  // origin, so after a few navigations every new request is queued indefinitely.
  const onPageHide = () => es.close();
  window.addEventListener('pagehide', onPageHide, { once: true });

  return () => {
    window.removeEventListener('pagehide', onPageHide);
    es.close();
  };
}
