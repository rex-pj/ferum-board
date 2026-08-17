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

## Image processing

Uploads are decoded, EXIF-rotated, cropped where the layout fixes an aspect,
downscaled and re-encoded before they are stored. Unlike everything else on this
page these are **site_config keys, not environment variables** — they live at
`/admin/settings` → Content Policy, and take effect on the next upload with no
restart.

| Key | Default | What it does |
| --- | --- | --- |
| `image_processing_enabled` | `true` | Master switch. Off stores uploads exactly as received. |
| `image_max_long_edge` | `2048` | Long-edge cap for images that are never cropped — post attachments, catalogue and plugin media. |
| `image_jpeg_quality` | `82` | Quality for every lossy re-encode. Clamped to 40–100. |

An **absent** row means enabled, not disabled: the seeder writes `true`, and an
install predating these keys should not be silently opted out.

Three sizes are **not** configurable, because they are frames the templates
depend on rather than preferences: avatar 512×512, cover 1600×400, thumbnail
1280×720 (`AVATAR_FRAME` / `COVER_FRAME` / `THUMBNAIL_FRAME` in
`constants.rs`). The cropper widget's `aspect` attribute is written to match
them; changing one without the other previews a framing the server does not
store.

`MAX_UPLOAD_MEGAPIXELS` (40) is likewise fixed, and deliberately: the
`MAX_*_BYTES` limits bound the *file*, which says nothing about what it decodes
to — a valid 400 KB PNG can declare 50000×50000 and ask for roughly 10 GB.
Exposing that as a setting would be exposing a denial-of-service switch.

### Uploads that are already correct are never decoded

Before anything else, the pipeline answers four questions from the file's
**header** — costing microseconds, against the hundreds of milliseconds a decode
costs:

* is it already the format this policy emits (JPEG for the lossy paths, PNG for
  the logo)?
* is it already within `image_max_long_edge`?
* is it under 300 KB?
* does it carry **no** EXIF?

All four yes, and no crop was requested, and the upload is stored exactly as
received. This matters most on the hardware this ships on — a shared-core VM,
where sustained image work exhausts burst capacity long before it costs money.

Two things it deliberately gives up. A 299 KB photo that *would* have compressed
to 150 KB is stored at 299 KB, and the loss is bounded by that threshold by
construction. And a file carrying EXIF is **never** fast-pathed however small,
because a re-encode is the only thing that strips the GPS coordinates a phone
writes there.

`FixedFrame` uploads — avatars, covers, thumbnails — are excluded outright:
the frame is the whole point, so no input is ever already correct.

### Uploads that are already small

**An upload is never replaced by something larger.** Re-encoding a source that
was already compressed harder than `image_jpeg_quality` makes it bigger, not
smaller — measured, a 1.48 MB q55 photo becomes 2.21 MB, and a 19 KB flat-colour
PNG becomes a 39 KB JPEG. When nothing about the image had to change — no crop,
no EXIF rotation, no downscale — and the re-encode would grow it, the original
bytes are stored instead.

Two consequences worth expecting:

* **The saving is concentrated on camera photos.** A forum whose uploads are
  mostly wallpapers or already-squeezed web images will see much of its traffic
  pass through untouched, and that is the correct outcome rather than a
  misconfiguration.
* **A file carrying EXIF is re-encoded even when that grows it**, because a
  re-encode is the only thing that strips it, and EXIF is where a phone writes
  GPS coordinates that `/files/` would then serve publicly. Privacy is chosen
  over bytes on that one branch, deliberately.

### Choosing a quality

82 is a reasonable default, not a researched one for your forum. The right
answer depends on subject matter: a photography board wants more, a support
board full of screenshots wants less. Measure it against your own images:

```powershell
$env:FERUM_IMAGE_CORPUS = "D:\a-folder-of-real-uploads"
cd backend
cargo test -p ferum-infrastructure-tests --profile release-fast -- --ignored --nocapture quality_sweep
```

It prints bytes out per quality per file plus the aggregate saving. Take the
**highest** quality whose saving is still acceptable — the size curve flattens
well before the image visibly improves.

Use `release-fast`, not `release`: the latter's fat LTO is the slowest step in
the build and buys nothing for a measurement. A plain debug build reports times
several times worse than production and is useless for the latency figures.

### Finding files nothing references

Reference counting is a write-path mechanism, so it holds only while every write
path plays along. Three did not — logo, favicon and theme previews each dropped
a reference without collecting the object — and two sources leak by design: a
post attachment that reaches zero is deliberately never deleted (a soft-deleted
post still references it, and when the image *is* the violation it is also the
evidence), and `InlineJobRunner` keeps its queue in memory, so a restart between
the decrement and the collection loses the job.

No write-path fix reaches any of that retroactively. The sweep does:

```bash
# Report. Never deletes. Page with ?after= until next_after comes back null.
curl -s 'https://forum.example.com/api/admin/storage/sweep?limit=500' -b cookies.txt

# Delete what it found. Same scan, re-run server-side.
curl -sX POST 'https://forum.example.com/api/admin/storage/sweep?limit=500' -b cookies.txt
```

`GET` reports and `POST` deletes — the **method** is the switch, not a query
flag. `middleware/csrf.rs` only inspects non-GET requests and `SameSite=Lax`
attaches the auth cookie to top-level navigations, so a destructive GET would be
one clicked link away from running.

Two fields to read carefully:

* `enumerable: false` means the configured backend **cannot list itself**, so
  nothing was examined. GCS is in this state today — it has no `list_keys`
  implementation. This is deliberately distinct from an empty result: reporting
  a clean store for a store nobody read would be the worst possible output here.
* `orphaned_objects` are deleted directly (no row exists, so the GC job would
  find nothing to check) while `uncollected` go through `GcStorageKey`, whose
  `ref_count = 0` re-test inside the DELETE is what stops a sweep removing
  something re-referenced while it ran.

**What it cannot find:** a row sitting at `ref_count = 1` that nothing actually
points at. That was the shape of the theme-preview leak, and it looks alive to
any check based on the count. Detecting it needs a per-namespace audit against
the six tables that hold file URLs — not built.

### Measured end to end

Run against a live instance on database storage, 2026-08-17. The fixtures come
from a generator so the numbers are reproducible:

```powershell
$env:FERUM_FIXTURE_OUT = "C:\tmp\ferum-fixtures"
cd backend
cargo test -p ferum-infrastructure-tests -- --ignored --nocapture emit_fixtures
```

| Upload | In | Out | |
| --- | --- | --- | --- |
| 4000×3000 photo → avatar | 1,666,986 B | 24,245 B, 512×512 | **1.5%**, EXIF gone |
| 1600×900, EXIF orientation 6 → attachment | 113,730 B | 144,081 B, **900×1600** | rotated upright, EXIF gone |
| 2-frame GIF → attachment | 88 B | 88 B | byte-identical, still animates |
| 600×600 PNG logo | 11,459 B | 12,748 B, 512×512 | PNG, alpha intact |
| 4000×3000 photo, processing **off** | 1,666,986 B | 1,666,986 B | byte-identical |

**Two of those grew, and both are correct.** The guard that prevents growth only
applies when nothing about the image had to change; a rotation and a downscale
are both real changes, so it stands aside and the output wins. The EXIF case
trades 27% more bytes for an image that is not sideways and carries no GPS. The
logo case is what moved `LOGO_MAX_LONG_EDGE` from 512 to 1024 — see that
constant.

> **Uploading a logo replaces the current one, and the old file is deleted.**
> Replacement drops the last reference, which schedules collection, and the
> bytes go. Expected behaviour, and worth knowing before you try this on an
> instance whose logo you want to keep.

### Latency

Encoding is CPU-bound, so upload endpoints carry their own budget — **p95 <
1200 ms** — carved out of NF-PF-04's 200 ms, which they cannot meet. Work runs
on `spawn_blocking` behind a semaphore sized `available_parallelism() - 1`, so
peak memory is bounded at `permits × 256 MB` and page renders never queue behind
an avatar upload.

```powershell
cargo test -p ferum-infrastructure-tests --profile release-fast -- --ignored --nocapture throughput
```

Needs no corpus — decode and encode cost scales with pixel count, not content.
If your hardware misses the budget, lower `image_max_long_edge` rather than
raising the budget: the number bounds how long one request may hold a permit.

### Turning it off

`image_processing_enabled = false` is a runtime switch. There is also a
compile-time one, `--no-default-features` or dropping the `image_processing`
cargo feature, which removes the codecs from the binary entirely and selects
`PassthroughImageProcessor`. Both degrade to the same behaviour: uploads stored
as received, which is what every path did before this existed.

**Neither is retroactive.** Originals are discarded at upload time, so turning
processing off changes what happens next and nothing about what is already
stored.

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
