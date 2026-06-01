import { redirect } from '@sveltejs/kit';
import type { Handle } from '@sveltejs/kit';
import { ROUTES } from '$lib/routes';
import { apiFetch } from '$lib/server/api';

const API_URL = process.env.API_URL ?? 'http://localhost:8080';

// Cached once setup is confirmed complete — avoids an extra API call on every request.
let setupComplete = false;

export const handle: Handle = async ({ event, resolve }) => {
	if (event.url.pathname.startsWith('/setup')) {
		return resolve(event);
	}

	if (!setupComplete) {
		try {
			const status = await apiFetch<{ data: { needs_setup: boolean } }>('/api/setup/status');
			if (status.data.needs_setup) {
				redirect(302, ROUTES.SETUP);
			} else {
				setupComplete = true;
			}
		} catch {
			// If the backend is unreachable, let the page handle the error
		}
	}

	// When the access token is gone but a refresh token is still present, silently
	// mint a new access token so the session survives the 1-hour expiry window.
	if (!event.cookies.get('token')) {
		const refreshToken = event.cookies.get('refresh_token');
		if (refreshToken) {
			try {
				const res = await fetch(`${API_URL}/api/auth/refresh`, {
					method: 'POST',
					headers: { Cookie: `refresh_token=${refreshToken}` }
				});
				if (res.ok) {
					const json = await res.json();
					const newToken: string | undefined = json.data?.access_token;
					if (newToken) {
						event.cookies.set('token', newToken, {
							httpOnly: true,
							sameSite: 'lax',
							secure: process.env.NODE_ENV === 'production',
							path: '/',
							maxAge: 3600
						});
					}
				} else {
					// Refresh token is expired or revoked — clear it so we stop retrying.
					event.cookies.delete('refresh_token', { path: '/' });
				}
			} catch {
				// Backend unreachable — proceed; pages will handle unauthenticated state.
			}
		}
	}

	return resolve(event);
};
