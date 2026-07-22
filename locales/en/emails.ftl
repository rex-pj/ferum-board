# Transactional email copy — English (source catalog).
#
# These are written in the *recipient's* language, resolved when the job is
# enqueued (see `ForumJob`), never the language of whoever triggered the action.
#
# `{ $url }` is interpolated as plain text into an href and a link label. Keep it
# on its own line so translators do not accidentally wrap it in punctuation that
# would end up inside the URL.

## ─── Email verification ──────────────────────────────────────────────────────

email-verify-subject = Verify your email
email-verify-body =
    <p>Welcome to { $site_name }! Click the link below to verify your email address:</p>
    <p><a href="{ $url }">{ $url }</a></p>
    <p>If you didn't create an account, you can ignore this email.</p>

## ─── Password reset ──────────────────────────────────────────────────────────

email-reset-subject = Reset your password
email-reset-body =
    <p>You asked to reset your password. Use the link below within one hour:</p>
    <p><a href="{ $url }">{ $url }</a></p>
    <p>If you didn't request this, you can safely ignore this email — your
    password will not change.</p>
