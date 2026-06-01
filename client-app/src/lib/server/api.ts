import type { Cookies } from '@sveltejs/kit';

const API_URL = process.env.API_URL ?? 'http://localhost:8080';

export interface ApiResponse<T> {
	data: T;
}

export interface PagedApiResponse<T> {
	data: T[];
	meta: {
		total: number;
		page: number;
		per_page: number;
	};
}

export interface ApiError {
	error: {
		code: string;
		message: string;
	};
}

export class ApiException extends Error {
	constructor(
		public status: number,
		public code: string,
		message: string
	) {
		super(message);
	}
}

/**
 * Server-side fetch wrapper that forwards the auth cookie from the browser
 * request to the backend API. Never called from client-side code.
 */
export async function apiFetch<T>(
	path: string,
	options: RequestInit & { cookies?: Cookies; timeout?: number } = {}
): Promise<T> {
	const { cookies, timeout = 8000, ...fetchOptions } = options;

	const headers: Record<string, string> = {
		'Content-Type': 'application/json',
		...(fetchOptions.headers as Record<string, string>)
	};

	// Forward the auth cookie to the backend
	if (cookies) {
		const token = cookies.get('token');
		if (token) {
			headers['Authorization'] = `Bearer ${token}`;
		}
	}

	const controller = new AbortController();
	const timer = setTimeout(() => controller.abort(), timeout);

	let res: Response;
	try {
		res = await fetch(`${API_URL}${path}`, {
			...fetchOptions,
			headers,
			signal: controller.signal
		});
	} finally {
		clearTimeout(timer);
	}

	if (!res.ok) {
		let body: ApiError | null = null;
		try {
			body = await res.json();
		} catch {}
		throw new ApiException(
			res.status,
			body?.error?.code ?? 'unknown',
			body?.error?.message ?? `HTTP ${res.status}`
		);
	}

	if (res.status === 204) {
		return undefined as T;
	}

	return res.json() as Promise<T>;
}
