# Configuration reference

Long-form rationale for the environment variables in `backend/.env.example`.
That file stays terse — one to four lines per variable — and points here for
anything that needs a paragraph.

Every capability below follows the same rule: **the presence of a variable is
the toggle.** There is no separate on/off switch, and a missing variable selects
a documented fallback rather than an error.

---

## Email

### `FROM_EMAIL`

The sender address on every outgoing message, whichever provider is active.

There is no separate "from name" variable — put the display name in this one, in
RFC 5322 form:

```
FROM_EMAIL=Ferum Board <noreply@example.com>
```

It is validated at startup and a malformed value **aborts the boot**. That is
deliberate. It used to be parsed at send time, so a typo produced a process that
started cleanly, reported healthy, and failed every message with a generic
internal error — with one line in a background job's log as the only evidence.

### `SMTP_*`

Seeded into the `site_config` table on first startup. After that, edit them at
**admin/settings → Email**, which swaps the live transport with no restart.

TLS is inferred from the address, so there is no setting to get wrong:

| Address | Transport |
| --- | --- |
| loopback host | cleartext (MailHog presents no certificate) |
| port 465 | implicit TLS |
| anything else | STARTTLS required |

A relay that does not offer STARTTLS is refused rather than handed the
credentials in plaintext. One consequence for Compose-based dev setups:
`SMTP_HOST=mailhog` is not loopback, so it would demand TLS — keep `localhost`
and publish the port instead.

### `RESEND_API_KEY`

Presence is the toggle, and it **wins over the SMTP block** and over anything in
`site_config`.

Env-only on purpose: a key that can send mail as your domain does not belong in
a table the admin settings page renders. The cost is that rotating it needs a
restart.

Verify your sending domain in Resend first and make `FROM_EMAIL` an address on
it — an unverified domain is the most common cause of a rejected send.

---

## Secrets at rest

### `SECRET_ENCRYPTION_KEY`

64 hex characters (32 bytes). **Required once `APP_URL` is https** — the app
refuses to start without it. Optional on http, where its absence is a WARN.

Encrypts every secret this app stores in PostgreSQL:

| Value | Where |
| --- | --- |
| SMTP password | `site_config`, key `smtp_pass` |
| Webhook HMAC key | `webhooks.secret` |
| Plugin credentials | `plugins.config`, each field the plugin's manifest marks `secret = true` |

**What it protects:** someone who reads the database *without* reading this
environment — a dump, a backup on third-party storage, a leaked replica. It does
nothing against an attacker already on the host, where the key is readable.

Enabling it needs no migration: existing plaintext rows are read unchanged and
converted on the next start.

**Losing the key makes those values unrecoverable.** Nothing else in the
database is encrypted, so posts, users and threads are never at risk.

**Turning it on does not protect backups already taken.** The sweep updates rows
in place, so under MVCC the old plaintext tuples survive until `VACUUM`, and any
dump made earlier is unaffected. If a pre-encryption backup may have leaked,
rotate the SMTP password, every webhook secret and every plugin credential —
encrypting the current values does nothing for copies already elsewhere.

**A plugin credential is only encrypted if its manifest says so.** Which fields
are secret cannot be a fixed list here: plugin config is defined by whoever wrote
the plugin. See `plugin-system/plugin-developer-guide.md` § `secret = true`. A
plugin that omits the flag stores its credential in plaintext even with this key
set.

### `SECRET_ENCRYPTION_KEY_PREVIOUS`

Set only during a rotation, to the key being retired. Values are opened with the
current key first and then this one, and everything is re-sealed on start. Unset
it once the log reports the re-seal count. Full runbook: `deployment.md`.

---

## Storage backends

Precedence when more than one is configured — the newest variable wins, and
every loser is named in a WARN:

```
R2_ACCOUNT_ID  >  GCS_BUCKET  >  S3_ENDPOINT  >  database
```

All three object stores are **opt-in cargo features**. A stock build falls back
to database storage whatever these say, logging a WARN that names the missing
feature. Switching backends needs no data migration: `stored_files.data` is
nullable, so existing rows keep serving from `/files/` while new uploads store
only metadata.

### S3 / MinIO — `--features s3`

The bucket is addressed path-style, so the public URL is
`{endpoint}/{bucket}/{key}` unless `CDN_BASE_URL` overrides it. Path-style is
pinned because MinIO has no DNS for `{bucket}.{host}` and a bucket name
containing a dot breaks TLS under virtual-hosted addressing.

`S3_ENDPOINT` can also point at Google Cloud Storage's S3 interoperability
endpoint (`https://storage.googleapis.com`) with an HMAC key pair. That works,
but the keys are long-lived static secrets — `GCS_BUCKET` gets you keyless
Workload Identity instead.

### Google Cloud Storage — `--features gcs`

**The bucket must be readable by `allUsers`**, or every image renders broken
with nothing failing on the server:

```bash
gcloud storage buckets add-iam-policy-binding gs://YOUR_BUCKET \
    --member=allUsers --role=roles/storage.legacyObjectReader
```

**Use `legacyObjectReader`, not `objectViewer`.** `objectViewer` also grants
`storage.objects.list`, which lets anyone on the internet enumerate every object
in the bucket — defeating the only thing protecting a staged attachment, namely
that its CAS key is unguessable. `legacyObjectReader` grants `objects.get`
alone: readable if you already know the name, not discoverable.

With uniform bucket-level access on (the default for new buckets) that IAM
binding is the only mechanism — per-object ACLs are ignored. The app probes this
at startup and WARNs if an anonymous read is refused.

**Credentials.** Leave all three credential variables unset on GCE / GKE /
Cloud Run: the metadata server supplies the token and no secret exists anywhere
in the deployment (Workload Identity). Locally, `gcloud auth
application-default login` is picked up automatically. Highest precedence
first: `GCS_CREDENTIALS_JSON`, `GCS_CREDENTIALS_FILE`,
`GOOGLE_APPLICATION_CREDENTIALS`.

**`GCS_PREFIX` does not make a shared bucket safe.** The `allUsers` binding is
bucket-wide, so pointing this app at a prefix inside a bucket holding anything
else publishes that too. To share safely, create a **managed folder** named for
the prefix and bind the role on the folder rather than the bucket (requires
uniform bucket-level access). A dedicated bucket with no prefix is simpler.

### Cloudflare R2 — `--features r2`

Credentials come from an R2 API token (Cloudflare dashboard → R2 → Manage API
tokens): `R2_ACCESS_KEY` is its "Access Key ID", `R2_SECRET_KEY` its "Secret
Access Key".

**`R2_PUBLIC_BASE_URL` is mandatory and the app refuses to start without it.**

R2 is the one backend whose public origin cannot be derived. Its S3 API endpoint
(`https://<account>.r2.cloudflarestorage.com`) serves *signed* requests only, so
an anonymous browser fetch of an uploaded image is refused there. The URL this
app mints is written into avatars, site config and the stored HTML of every
post, and that text is never rewritten — so minting one under the API endpoint
would bake permanently-broken links into your content. Refusing to start is the
only reaction that cannot corrupt anything.

Get a public origin one of two ways, under R2 → your bucket → Settings → Public
access:

- **Connect a custom domain** (production). Served through the Cloudflare CDN
  with caching, WAF and bot management.
- **Enable the public development URL**, `https://pub-<hash>.r2.dev`. Cloudflare
  rate limits it and documents it as non-production; the app logs a warning when
  you point `R2_PUBLIC_BASE_URL` at one.

`CDN_BASE_URL` is **not** a substitute. It is still read on the way back in, so
URLs written while this bucket was served through the S3 adapter keep resolving,
but it never mints.

### `CDN_BASE_URL`

Honoured by every backend, shaped differently by each: `{cdn}/files/{key}` on
database storage, `{cdn}/{key}` on S3 and GCS. No trailing slash.

Unset is a supported configuration, not a broken one — each backend then uses
its own origin.

**Removing it later orphans URLs written while it was set.** The absolute
`{cdn}/…` form is only recognised on the way back in while the value is present,
and that recognition drives CAS reference counting. Changing it is cheap;
unsetting it is not.

---

## Logging

`RUST_LOG` and `LOG_LEVEL` are the **same filter from two sources, and
`RUST_LOG` wins outright**: `main.rs` tries `EnvFilter::try_from_default_env()`
first and only falls back to `LOG_LEVEL` when `RUST_LOG` is absent. Setting both
and then editing `LOG_LEVEL` is a change with no effect — the usual reason a
log-level tweak "does nothing". Comment `RUST_LOG` out to make `LOG_LEVEL` the
live knob.

`ferum_web` is the **library** crate (handlers, middleware, render logic) and
its tracing target is `ferum_web::*`, separate from the binary entry point
`ferum_board`. Both must appear in the filter or web-layer errors and warnings
are silently dropped.
