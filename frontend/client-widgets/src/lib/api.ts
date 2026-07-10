async function request(url: string, init: RequestInit = {}): Promise<Response> {
  const headers: Record<string, string> = { ...(init.headers as Record<string, string> | undefined) };
  if (init.body !== undefined && !headers['Content-Type']) {
    headers['Content-Type'] = 'application/json';
  }
  return fetch(url, { ...init, headers });
}

export const api = {
  get:      (url: string)                 => request(url, { method: 'GET' }),
  post:     (url: string, body?: unknown) => request(url, { method: 'POST',   body: body != null ? JSON.stringify(body) : undefined }),
  put:      (url: string, body?: unknown) => request(url, { method: 'PUT',    body: body != null ? JSON.stringify(body) : undefined }),
  delete:   (url: string)                 => request(url, { method: 'DELETE' }),
  // No Content-Type header — the browser sets multipart/form-data with the boundary itself.
  postForm: (url: string, formData: FormData) => fetch(url, { method: 'POST', body: formData }),
};
