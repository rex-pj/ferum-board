# Discord Notifier — Ferum Plugin (Tier 1)

Posts a Discord embed to a channel webhook when new threads or posts are created.

## Install

1. Upload `discord-notifier-1.0.0.fpkg` via **Admin → Plugins → Install Plugin**.
2. Review the requested capabilities (outbound webhook only) and confirm.
3. Go to **Admin → Plugins → Discord Notifier → Configuration**.
4. Paste your Discord Webhook URL and save.
5. Click **Enable** to activate.

## How it works

This is a **Tier 1 (Manifest)** plugin — the simplest possible plugin type.  
Ferum registers the webhook URL from your config and calls it (via the existing  
webhook fan-out system) whenever a `post.created` event fires.

No code runs on your server. Ferum just makes an outbound HTTP POST to Discord.

## Discord Webhook format

Ferum sends the standard webhook payload (see `WebhookSubscriber`).  
Discord accepts arbitrary JSON — if you need a custom embed format, use a  
**Tier 2 (Script)** plugin instead, which can transform the payload before sending.

## Config fields

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `webhook_url` | ✓ | — | Discord channel webhook URL |
| `notify_on_post` | | `false` | Also notify on replies (not just new threads) |
| `username` | | `Ferum Board` | Bot display name in Discord |
