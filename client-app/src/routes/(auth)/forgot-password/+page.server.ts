import { fail } from '@sveltejs/kit';
import type { Actions } from './$types';

export const actions: Actions = {
	default: async ({ request, fetch }) => {
		const data = await request.formData();
		const email = data.get('email') as string;

		if (!email) return fail(422, { error: 'Email is required.' });

		await fetch(
			`${process.env.API_URL ?? 'http://localhost:8080'}/api/auth/password-resets`,
			{
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ email })
			}
		);

		// Always return success to prevent user enumeration
		return { success: true };
	}
};
