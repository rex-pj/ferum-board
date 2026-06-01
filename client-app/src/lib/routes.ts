export const ROUTES = {
	HOME: '/',

	// Setup wizard (first-run only)
	SETUP: '/setup',

	// Public
	FORUM_INDEX: '/forum',
	CATEGORY: (slug: string) => `/forum/${slug}`,
	THREAD: (slug: string) => `/forum/t/${slug}`,
	SEARCH: '/search',
	USER_PROFILE: (username: string) => `/u/${username}`,

	// Auth (unauthenticated only)
	LOGIN: '/login',
	REGISTER: '/register',
	FORGOT_PASSWORD: '/forgot-password',
	RESET_PASSWORD: '/reset-password',
	LOGOUT: '/logout',

	// App (authenticated)
	NEW_THREAD: '/new-thread',
	NEW_THREAD_IN_CATEGORY: (categoryId: string) => `/new-thread?category=${categoryId}`,
	EDIT_THREAD: (slug: string) => `/edit-thread/${slug}`,
	ACCOUNT: '/account',
	NOTIFICATIONS: '/notifications',
	BOOKMARKS: '/bookmarks',

	// Mod
	MOD: {
		REPORTS: '/mod/reports',
		THREADS: '/mod/threads',
		USER: (username: string) => `/mod/users/${username}`,
		LOG: '/mod/log'
	},

	// Admin
	ADMIN: {
		DASHBOARD: '/admin/dashboard',
		USERS: '/admin/users',
		USER: (id: string) => `/admin/users/${id}`,
		CATEGORIES: '/admin/categories',
		CATEGORY: (id: string) => `/admin/categories/${id}`,
		CATEGORY_MODS: (id: string) => `/admin/categories/${id}/moderators`,
		THREADS: '/admin/threads',
		REPORTS: '/admin/reports',
		LOG: '/admin/log',
		SETTINGS: '/admin/settings'
	},

	// Media files (CAS — content-addressed, cache-immutable)
	FILE: (key: string) => `/files/${key}` as const,
} as const;
