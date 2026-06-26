/**
 * Ferum Profanity Filter — Tier 2 Script Plugin
 *
 * Hooks:
 *   before_post_create   — ctx.payload: { content_md, thread_id }
 *   before_thread_create — ctx.payload: { title, content_md, category_id }
 *
 * Return values:
 *   { allow: true }                                    — pass through
 *   { deny: { reason: "...", error_code: "..." } }     — block with 403
 */
(function () {
    'use strict';

    // ── Helpers ──────────────────────────────────────────────────────────────

    /** Parse the comma-separated blacklist from config into a trimmed array. */
    function getBlacklist() {
        var raw = (Ferum.config && Ferum.config.blacklist) || '';
        return raw
            .split(',')
            .map(function (w) { return w.trim().toLowerCase(); })
            .filter(function (w) { return w.length > 0; });
    }

    /**
     * Scan text for any blacklisted word.
     * Returns the first matched word, or null if clean.
     */
    function findBannedWord(text, blacklist) {
        if (!text || blacklist.length === 0) return null;
        var lower = text.toLowerCase();
        for (var i = 0; i < blacklist.length; i++) {
            if (lower.indexOf(blacklist[i]) !== -1) {
                return blacklist[i];
            }
        }
        return null;
    }

    // ── Core check ───────────────────────────────────────────────────────────

    function checkContent(ctx) {
        var blacklist = getBlacklist();

        // No words configured → always allow
        if (blacklist.length === 0) {
            return { allow: true };
        }

        var payload = ctx.payload || {};
        var title      = payload.title      || '';
        var contentMd  = payload.content_md || '';

        var found = findBannedWord(title, blacklist) || findBannedWord(contentMd, blacklist);

        if (found) {
            var shouldLog = Ferum.config.log_blocked !== false;
            if (shouldLog) {
                Ferum.log.warn('Profanity blocked', {
                    word: found,
                    hook: ctx.hook_name,
                    actor_id: ctx.actor_id || null,
                    trust_level: ctx.actor_trust_level,
                });
            }

            var reason = (Ferum.config && Ferum.config.block_message)
                || 'Your post contains prohibited content. Please review our community guidelines.';

            return {
                deny: {
                    reason: reason,
                    error_code: 'profanity_blocked',
                },
            };
        }

        return { allow: true };
    }

    // ── Register hooks ────────────────────────────────────────────────────────

    __ferum_hooks['before_post_create']   = checkContent;
    __ferum_hooks['before_post_edit']     = checkContent;
    __ferum_hooks['before_thread_create'] = checkContent;

    Ferum.log.info('Profanity Filter loaded — ' +
        getBlacklist().length + ' word(s) in blacklist');
})();
