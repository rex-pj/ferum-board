# Security Audit §5.4 — Data-at-Rest Classification

> Result of the audit defined by [security-audit-checklist.md](security-audit-checklist.md)
> §5.4. Scope: every column in `backend/migration/` and
> `backend/crates/ferum-infrastructure/src/entities/`.
>
> **Audit date:** 2026-08-15 · **Commit audited:** `fe8c028` (branch `develop`)
>
> **All four findings were fixed on the same day.** The table below shows the state
> *after* those fixes; each finding records what it was and what changed. The
> history is kept rather than deleted — a checklist that only ever shows PASS
> teaches nothing about which mistakes this codebase is prone to.
>
> Re-run this when a migration adds a column, when a plugin gains a config field
> that holds a credential, or before a release that touches auth, webhooks, or
> plugin configuration.

---

## Why this section exists separately from 5.1–5.3

§5.1 checks that passwords are hashed, §5.2 that the algorithms are current, §5.3
that token randomness comes from an OS entropy source. All three pass here, and
none of them looks at any other column. A credential stored in a JSONB blob, or a
PII field written into logs, is invisible to those three checks and would sail
through the rest of the audit untouched — which is exactly what findings 2 and 4
below turned out to be.

---

## Classification table

| Column | Table | Classification | Current state | Verdict | Fix |
| --- | --- | --- | --- | --- | --- |
| `password_hash` | `users` | must be hashed | bcrypt, `BCRYPT_COST` = 12; the only call site is `bcrypt_password_hasher.rs` | **PASS** | — |
| `value` where `key='smtp_pass'` | `site_config` | must be encrypted | XChaCha20-Poly1305 AEAD; `SECRET_ENCRYPTION_KEY` now **required on https** | **PASS** | [Finding 1](#finding-1--encryption-was-opt-in-and-off-by-default) |
| `secret` | `webhooks` | must be encrypted | same cipher, same requirement (`webhook_repository.rs`) | **PASS** | [Finding 1](#finding-1--encryption-was-opt-in-and-off-by-default) |
| `config` (JSONB) | `plugins` | must be encrypted | sealed per field via manifest `secret = true` (`plugin_repository.rs`) | **PASS** | [Finding 2](#finding-2--pluginsconfig-held-bearer-credentials-in-plaintext) |
| `value` (JSONB) | `plugin_storage` | unclassifiable — plugin-authored | plaintext, documented as unsuitable for credentials | **PASS (by policy)** | [Finding 3](#finding-3--plugin_storagevalue-is-unbounded-plugin-written-jsonb) |
| `email` | `users` | masked in logs and API responses | plaintext in DB (correct); logs carry `email_log_key` — digest + domain | **PASS** | [Finding 4](#finding-4--full-email-address-written-to-structured-logs-on-failed-login) |
| `context` (JSONB) | `plugin_logs` | masked in logs | plugin-supplied; guide forbids logging credentials | **PASS (by policy)** | [Finding 3](#finding-3--plugin_storagevalue-is-unbounded-plugin-written-jsonb) |
| `metadata` (JSONB) | `audit_logs` | fine as plaintext | ban/warn reasons and ids only — no credentials | PASS | — |
| `ban_reason` | `users` | fine as plaintext | plaintext, access-controlled | PASS | — |
| `reason` | `reports` | fine as plaintext | plaintext, access-controlled | PASS | — |
| `data` (BYTEA) | `stored_files` | fine as plaintext | plaintext; `/files/` authorizes staged rows per viewer | PASS | — |
| `payload` (JSONB) | `notifications` | fine as plaintext | usernames, thread titles | PASS | — |
| `manifest`, `granted_capabilities` (JSONB) | `plugins` | fine as plaintext | declarative, no credentials | PASS | — |
| IP addresses | — | masked + retention policy | **never persisted** — used only as rate-limit cache keys, TTL-bound | PASS (N/A) | — |
| Resend API key | — | must be encrypted | env-only, never a column; `[redacted]` in the `Debug` impl | PASS | — |
| Encryption key location | — | must live outside the DB | `SECRET_ENCRYPTION_KEY` environment variable | PASS | — |

Only two columns in the entire schema are *named* like secrets — `users.password_hash`
and `webhooks.secret`. Everything else sensitive is either row-keyed (`site_config`)
or JSONB, so a grep over column names alone finds neither the SMTP password nor the
plugin credentials. Any future re-run of this audit has to enumerate columns, not
match names.

---

## Findings

### Finding 1 — Encryption *was* opt-in, and off by default

**Was:** `SECRET_ENCRYPTION_KEY` was commented out in `backend/.env.example`, so a
stock deployment stored `smtp_pass` and every webhook HMAC key as plaintext. The
startup log carried a WARN and `/admin/settings` showed a badge — visible, but a
warning is exactly what let a production install run for two days without the key,
because nobody reads startup logs on a deploy that came up healthy.

**Fixed:** an https `APP_URL` without the key now **aborts startup**, with a message
carrying the `openssl rand -hex 32` command and the `--force-recreate` caveat. http
keeps the warning — that is a laptop, and forcing a key there buys nothing but
friction.

The check deliberately does **not** first ask whether the database already holds a
secret. Firing on the empty database, at first deploy, is the whole point: gating on
existing secrets would let the boot succeed right up until the operator saves SMTP
settings, which is the worst possible moment to learn about a required variable.

The mechanism itself was already sound and needed no change:

- XChaCha20-Poly1305 AEAD — authenticated, which matters because one protected
  value *is* an HMAC key; an unauthenticated cipher would yield 32 bytes of garbage
  that the dispatcher would then sign with, failing silently at every subscriber.
- AAD binds each ciphertext to the column it lives in (`site_config:smtp_pass`,
  `webhooks.secret`), so a value moved between rows fails authentication instead of
  decrypting.
- The `enc:v1:` prefix versions the envelope, so a future algorithm can coexist
  row by row with no data migration.
- Plaintext reads back unchanged and is sealed on next write, making adoption lazy
  rather than a flag day.
- Rotation is `SECRET_ENCRYPTION_KEY_PREVIOUS`, with a re-seal sweep at startup.
- A key that does not match existing data **aborts the boot**, rather than starting
  and re-sealing under the wrong key — which would destroy the originals.

Two limits worth stating plainly, because neither is a defect but both change what
"encrypted" buys you:

- It protects a stolen dump, backup, or replica. It does **not** protect against an
  attacker on the host, where the key is readable.
- `seal_existing_secrets` updates in place. Under Postgres MVCC the old plaintext
  tuples survive until `VACUUM`, and backups taken earlier are unaffected. Enabling
  encryption today does not retroactively protect a dump taken yesterday — rotate
  the SMTP password and every webhook secret if a pre-encryption backup may have
  leaked.

### Finding 2 — `plugins.config` held bearer credentials in plaintext

**Was:** `examples/plugins/discord-notifier/plugin.toml` declares `webhook_url` as a
required config field. That URL *is* a credential — anyone holding it can post into
the target channel, with no further authentication. It was stored as plaintext JSONB,
landing in every `pg_dump` alongside the two values the cipher did cover.

(The API exposure — `GET /api/admin/plugins/:slug` returns the config in full — is
*not* part of this finding and was left alone. It requires `admin.plugins`, and the
config editor is a raw JSON textarea by design, so the value has to reach the
browser.)

**Fixed:** a manifest field may now carry `secret = true`, and
`PgPluginRepository` seals exactly those fields.

`ENCRYPTED_CONFIG_KEYS` could not be extended to cover this. Site config has a closed
set of keys defined in this repository; plugin config is defined by whoever wrote the
plugin, so the manifest is the only place the answer can live. `secret_config_keys`
(in `ferum-domain`) reads the flag; the repository seals on write and opens on read,
with the same plaintext-passes-through behaviour the site-config path already had, so
adding the flag to an installed plugin is safe.

The AAD binds each ciphertext to **both the plugin slug and the field name**. Binding
only the field would let someone with database access graft one install's credential
onto another plugin — or onto a second plugin that happens to name a key `api_key` —
and have it decrypt cleanly into a context its owner never granted. The slug is used
rather than the plugin's UUID so a value survives an uninstall/reinstall cycle, which
mints a new id for what the operator considers the same plugin.

The seam is entity→domain, matching `PgWebhookRepository` and for the same reason:
every consumer of a plugin credential — the Tier 1 webhook templater substituting
`{{config.webhook_url}}`, the Tier 2 script runtime exposing `Ferum.config`, the admin
page — reads it through `Plugin`. Decrypting further up would mean auditing each
consumer separately, and a missed one fails silently: a webhook POSTed to the literal
string `enc:v1:…` is a 404 at the far end, recorded as an ordinary delivery failure.

**The residual risk is real and stated rather than hidden:** a plugin author who omits
the flag gets no protection. Ferum cannot infer which fields are credentials. This is
the same trade as a plugin that declares no capabilities, and the reason the flag is
documented beside `required`, where it is hard to miss.

### Finding 3 — `plugin_storage.value` is unbounded plugin-written JSONB

**Was:** nothing constrained what a plugin persisted there, including an OAuth token,
and nothing told plugin authors not to. The same applies to `plugin_logs.context`,
which is stored verbatim and shown in the admin UI.

**Fixed as policy, not as code.** Neither column can be encrypted the way
`plugins.config` now is: the values are arbitrary JSON written at runtime, and nothing
declares their shape in advance, so there is no equivalent of the manifest flag to
read. The developer guide now states plainly that plugin storage and plugin logs are
**not** encrypted, that namespacing keeps other plugins out but does nothing about a
database dump, and that a credential a plugin needs to keep belongs in a
`secret = true` config field instead.

No shipped example stores a credential in either, so this closes as a documented
constraint rather than a live defect.

### Finding 4 — Full email address written to structured logs on failed login

**Was:** `backend/crates/ferum-application/src/usecases/auth_usecase.rs`, in the
unknown-email branch of `login`:

```rust
tracing::warn!(email = %cmd.email.to_lowercase(), "login failed: unknown email");
```

Every other branch of that function logs `user_id` instead. This one cannot — no
user was found — which is precisely how the exception arose. Two consequences:

- PII flowed into log aggregation on every mistyped address, and a login endpoint
  is exactly where the addresses of people who do *not* have accounts show up.
- Users routinely paste a password into the email field, so the log could capture a
  credential for an account that does exist under a different address.

**Fixed:** the line now goes through `email_log_key` (`ferum-application/src/shared.rs`),
which renders `<12 hex of SHA-256>@<domain>`.

Why that shape rather than a simple mask:

- The digest keeps what the line is *for* — telling one source probing many distinct
  addresses apart from repeated attempts against one. Domain-only would have lost
  that distinction, and it inverts the conclusion drawn from the log.
- The domain stays in the clear because "someone is enumerating @ourcorp.com" is an
  operationally different event from scattered noise, and a domain shared by millions
  of mailboxes identifies nobody on its own.
- A masked local part (`a***@example.com`) was rejected: the leading characters of a
  pasted password are still a meaningful head start.
- A value with no `@` — the pasted-password case — reduces to the bare digest, so
  nothing recognisable survives, not even the shape of what was typed.

Covered by `tests/application/src/shared.rs`, which asserts the properties rather
than a fixed string: the local part never survives, the domain does, distinct
addresses stay distinct, and the same address normalises to the same key.

---

## Production status of `SECRET_ENCRYPTION_KEY`

The `.env.prod` bootstrap in `docs/deployment.md` **does** generate the key today
(added 2026-08-14 in `3ea6645`), and `deploy/docker-compose.prod.yml` passes it
through via `env_file: .env.prod` — it is not shadowed by the explicit
`environment:` block, which wins over `env_file` for the keys it names.

**A server bootstrapped before 2026-08-14 does not have it.** The deployment
runbook as of `29d4d35` (2026-08-13) contained no such line, so any `.env.prod`
written from that revision is missing the variable and the site is storing its SMTP
password and webhook secrets in plaintext.

Since the Finding 1 fix, such a server **will not start** on the next deploy — the
https check aborts the boot. That is the intended outcome and not a regression to
work around: a container that refuses to start with an actionable message is
recoverable in one command, and it is the only signal that reliably gets read.

Verify on the host:

```bash
sudo docker compose -f /opt/ferum/deploy/docker-compose.prod.yml logs app \
  | grep -i "secrets at rest"
```

`Secrets at rest: encrypted (SECRET_ENCRYPTION_KEY is set)` means it is on. The WARN
about plaintext means it is not. To add it to a running deployment — append, never
re-run the `tee` block, which truncates and would regenerate `JWT_SECRET` and
`DB_PASSWORD`:

```bash
cd /opt/ferum
echo "SECRET_ENCRYPTION_KEY=$(openssl rand -hex 32)" | sudo tee -a .env.prod
sudo docker compose --env-file .env.prod -f docker-compose.prod.yml up -d --force-recreate app
```

`--force-recreate`, not `restart`: `env_file` is read when a container is *created*,
so a restart reuses the old environment and the new variable appears to do nothing.
Existing plaintext rows are converted on that start. Back up `.env.prod` — losing
the key makes those two values unrecoverable, and nothing else in the database is
encrypted.

---

## Out of scope but noted

`PUT /api/admin/config` writes no `audit_logs` row. Good for this section — no
plaintext copy of a secret lands in the audit table — but a gap against
[§11 Insufficient Logging & Monitoring](security-audit-checklist.md#11-insufficient-logging--monitoring).
