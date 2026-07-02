/**
 * Ferum Moderation Action Guard — Tier 2 Script Plugin
 *
 * Hooks:
 *   before_post_delete   — ctx.payload: { post_id, thread_id, category_id, author_id }
 *   before_thread_delete — ctx.payload: { thread_id, category_id, author_id }
 *   before_user_ban      — ctx.payload: { target_user_id, reason, until, permanent }
 *
 * Return values:
 *   { allow: true }                                    — pass through
 *   { deny: { reason: "...", error_code: "..." } }     — block with 403
 */
(function () {
    'use strict';

    // ── Helpers ──────────────────────────────────────────────────────────────

    function shouldLogDeletes() {
        return Ferum.config.log_deletes !== false;
    }

    function requiresBanReason() {
        return Ferum.config.require_ban_reason !== false;
    }

    // ── Hook handlers ───────────────────────────────────────────────────────

    function logPostDelete(ctx) {
        var payload = ctx.payload || {};
        if (shouldLogDeletes()) {
            Ferum.log.info('Post deleted', {
                post_id: payload.post_id,
                thread_id: payload.thread_id,
                actor_id: ctx.actor_id || null,
            });
        }
        return { allow: true };
    }

    function logThreadDelete(ctx) {
        var payload = ctx.payload || {};
        if (shouldLogDeletes()) {
            Ferum.log.info('Thread deleted', {
                thread_id: payload.thread_id,
                category_id: payload.category_id,
                actor_id: ctx.actor_id || null,
            });
        }
        return { allow: true };
    }

    function guardBan(ctx) {
        var payload = ctx.payload || {};
        var reason = (payload.reason || '').trim();

        if (requiresBanReason() && reason.length === 0) {
            Ferum.log.warn('Ban blocked — empty reason', {
                target_user_id: payload.target_user_id,
                actor_id: ctx.actor_id || null,
            });
            return {
                deny: {
                    reason: 'A ban reason is required.',
                    error_code: 'ban_reason_required',
                },
            };
        }

        Ferum.log.info('User ban approved', {
            target_user_id: payload.target_user_id,
            permanent: payload.permanent,
            actor_id: ctx.actor_id || null,
        });
        return { allow: true };
    }

    // ── Register hooks ────────────────────────────────────────────────────────

    __ferum_hooks['before_post_delete']   = logPostDelete;
    __ferum_hooks['before_thread_delete'] = logThreadDelete;
    __ferum_hooks['before_user_ban']      = guardBan;

    Ferum.log.info('Moderation Action Guard loaded');
})();
