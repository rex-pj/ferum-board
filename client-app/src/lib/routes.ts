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
	USER_POSTS: (username: string) => `/u/${username}/posts`,
	USER_FOLLOWERS: (username: string) => `/u/${username}/followers`,
	USER_FOLLOWING: (username: string) => `/u/${username}/following`,

	// Auth (unauthenticated only)
	LOGIN: '/login',
	REGISTER: '/register',
	FORGOT_PASSWORD: '/forgot-password',
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
		QUEUE: '/mod/queue',
		THREADS: '/mod/threads',
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
		SETTINGS: '/admin/settings',
		ROLES: '/admin/roles',
		ROLE: (id: string) => `/admin/roles/${id}`,
		PERMISSIONS: '/admin/permissions',
		PLUGINS: '/admin/plugins',
		PLUGIN: (slug: string) => `/admin/plugins/${slug}`
	},
} as const;
