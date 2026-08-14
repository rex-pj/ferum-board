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

## ─── Notification email ──────────────────────────────────────────────────────
#
# Sent only to members who opted in (see `EmailNotificationPrefs`), and only for
# these two kinds — reactions and follows stay in-app.
#
# Every one of these MUST carry `{ $unsubscribe_url }`. A notification email with
# no working one-click opt-out is the message people report as spam rather than
# mute, and that cost falls on the sending domain, not on the forum.

email-notify-reply-subject = { $actor } replied to "{ $thread_title }"
email-notify-reply-body =
    <p><strong>{ $actor }</strong> replied to your thread <strong>{ $thread_title }</strong> on { $site_name }.</p>
    <p><a href="{ $url }">Read the reply</a></p>
    <hr>
    <p style="font-size:12px;color:#666">
      <a href="{ $unsubscribe_url }">Unsubscribe from these emails</a> ·
      <a href="{ $settings_url }">Notification settings</a>
    </p>

email-notify-mention-subject = { $actor } mentioned you in "{ $thread_title }"
email-notify-mention-body =
    <p><strong>{ $actor }</strong> mentioned you in <strong>{ $thread_title }</strong> on { $site_name }.</p>
    <p><a href="{ $url }">View the post</a></p>
    <hr>
    <p style="font-size:12px;color:#666">
      <a href="{ $unsubscribe_url }">Unsubscribe from these emails</a> ·
      <a href="{ $settings_url }">Notification settings</a>
    </p>

## ─── Admin test send ─────────────────────────────────────────────────────────
#
# Sent only by the "Send test email" button in /admin/settings, and only ever to
# the acting admin's own address. Unlike the messages above this one has no link,
# because its whole content is the fact that it arrived.

email-test-subject = Test email from { $site_name }
email-test-body =
    <p>Mail delivery from { $site_name } is working.</p>
    <p>This message was sent through the <strong>{ $provider }</strong> provider
    by the "Send test email" button in the admin settings.</p>
