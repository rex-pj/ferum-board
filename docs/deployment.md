# Deployment — CI/CD to Google Compute Engine

How Ferum Board is built, shipped and run in production: one GCE VM behind
Cloudflare, images built by GitHub Actions, everything on the box in Docker
Compose. Around US$32/month.

Nothing here is specific to one installation. Commands are written against the
shell variables set in **Values you choose** below — export them once per shell
and the blocks are copy-pasteable as they stand. Anywhere a literal appears
(`example.com`, `sa_<uniqueId>`), it is marked as a placeholder.

This is one known-good arrangement, not the only one. The parts that generalise
badly are called out where they arise.

---

## The shape of it

```
git push (develop)
     │
     ▼
GitHub Actions ── .github/workflows/ci.yml
     │              ├─ test (two timezones) ─┐
     │              ├─ clippy + machete ─────┤ both must pass
     │              └─ image ◄───────────────┘
     │                   └─ docker build (Dockerfile, repo root) ~15 min
     │                   └─ push → Artifact Registry
     │                 └─ release
     │                       └─ ssh (IAP) → sudo ferum-deploy <sha>   ~1 min
     ▼
   Cloudflare (proxy, TLS to the browser)
     │  only Cloudflare edge IPs may reach :443
     ▼
   GCE VM  e2-medium · 2 vCPU · 4 GB · 30 GB pd-balanced
     └─ nginx ──► app ──► postgres
                    ├──► redis
                    └──► minio
```

**Deploy is a job inside CI, not its own workflow.** It is gated on
`github.event_name == 'push' && github.ref == 'refs/heads/develop'`, so a
`pull_request` run stops after the tests. The obvious alternative — a separate
`deploy.yml` keyed on `workflow_run: [CI]` — was tried and never fired once:
GitHub triggers `workflow_run`, and offers `workflow_dispatch`, only for a
workflow file present on the **default branch**, which is `main` here, and `main`
carries no workflows. A `push` runs the file from the branch pushed, so this
arrangement needs nothing on `main`.

**The VM never compiles anything.** `[profile.release]` uses `lto = "fat"` with
`codegen-units = 1`; rustc peaks at 4–8 GB on the final unit, against 4 GB of
host RAM. Builds happen on a GitHub runner (4 vCPU / 16 GB) and arrive as an
image.

### Values you choose

Export these at the top of every shell you use — Cloud Shell and the VM both.
Every command below is written against them.

```bash
export PROJECT=my-gcp-project          # GCP project ID
export VM=my-forum-vm                  # Compute Engine instance name
export ZONE=us-central1-a              # instance zone
export REGION=us-central1              # region: Artifact Registry, static IP
export REPO=my-org/my-fork             # GitHub owner/repo holding this code
export DOMAIN=example.com              # the site's public hostname
```

Three more values are *produced* by the setup rather than chosen, and are needed
later. Phase A prints all three.

| | Where it comes from | Used by |
| --- | --- | --- |
| `$IP` | the static address reserved in Phase A | the Cloudflare A records |
| `sa_<uniqueId>` | the CI service account's OS Login username | the sudoers entry in Phase E |
| `WIF_PROVIDER` | the Workload Identity provider path | a GitHub secret, Phase B |

Fixed regardless of installation: the app lives at `/opt/ferum` on the VM,
root-owned; the CI identity is `gh-deploy@$PROJECT.iam.gserviceaccount.com`; the
deploy branch is `develop`; the registry path is
`$REGION-docker.pkg.dev/$PROJECT/ferum/ferum-board`.

> **Keep your own values in a file this repository does not track.** A runbook
> with one installation's project ID and hostname baked in is unusable by anyone
> else, and re-deriving them every session is its own tax. `docs/` is gitignored
> for `deployment.*.md`, so `docs/deployment.mysite.md` holding the six exports
> above plus whatever is peculiar to your setup stays local. None of it is
> secret — a project ID and a hostname are public — the point is that the
> procedure and the particulars live apart.

---

## Read this before running anything

Three facts account for most of the time lost bringing this deployment up. None
of them produce an error that names the actual cause.

### 1. There are two shells, and they are not interchangeable

**Cloud Shell** and the **VM** are both Linux, both have `sudo`, and both have
`gcloud`. Nothing in the prompt announces which is which, and running a phase in
the wrong one fails in a way that reads like a broken setup rather than a wrong
window.

```
jane@cloudshell:~ (my-gcp-project)$     ← Cloud Shell
jane_example_com@my-forum-vm:~$         ← the VM
    the part after @ is the answer ────┘
```

`hostname` settles it without reading anything: Cloud Shell answers `cs-…`, the
VM answers its instance name.

| Runs only in Cloud Shell | Runs only on the VM |
| --- | --- |
| `gcloud services enable`, `gcloud iam …`, `gcloud compute ssh/scp` — anything reaching a Google API | `sudo install`, `sudo tee`, `docker`, `ferum-deploy` — anything touching the box |

Each side has a distinctive failure, and both are more useful than they look:

* **On the VM, an API call fails with `Request had insufficient authentication
  scopes`** — gcloud there authenticates as the VM's own service account under
  default scopes. Cloud Shell never produces this. Seeing it means: you are on
  the VM, press on with the local commands.
* **In Cloud Shell, a VM command fails with `No such file or directory` or
  `unknown user sa_…`** — every one of those answers is a truthful description of
  Cloud Shell. It says nothing at all about the VM.

A third window exists and is the easiest to reach: **SSH-in-browser**, from the
Console's VM instances list. That is the VM.

### 2. Do the phases in order — A really does come before D

Phase A enables OS Login, and **OS Login changes your own Unix username** (see
the note in Phase D). Provisioning the box first and enabling OS Login afterwards
leaves the `docker` group membership and every file you created owned by an
account you no longer log in as.

### 3. Verify at the end of each phase, not at the end

The pipeline takes ~15 minutes to reach the VM, so a missing step found by CI
costs a full run to retry. Every phase below ends with a check that costs
seconds. The Phase E preflight is the important one — run it before pushing.

---

## Phase A — GCP, once per project

Run in **Cloud Shell** (the `>_` icon in the Console), not over SSH into the VM.
The VM's service account has default access scopes and will refuse most of these
with `ACCESS_TOKEN_SCOPE_INSUFFICIENT`.

Safe to re-run: resources that exist report `ALREADY_EXISTS` and are skipped.

```bash
#!/bin/bash
# PROJECT / VM / ZONE / REGION / REPO come from "Values you choose" above.
# Export them in this shell first, or the block below creates the wrong things
# in the wrong places without complaining.
: "${PROJECT:?set PROJECT first}" "${VM:?}" "${ZONE:?}" "${REGION:?}" "${REPO:?}"
export ADDR=ferum-ip-$VM

gcloud config set project $PROJECT

# --- Enable APIs ---------------------------------------------------
# gcloud offers to enable Artifact Registry interactively but stays
# silent about the rest, so they are named here.
gcloud services enable \
  compute.googleapis.com \
  iam.googleapis.com \
  iamcredentials.googleapis.com \
  artifactregistry.googleapis.com \
  iap.googleapis.com

# --- Static IP -----------------------------------------------------
# An ephemeral IP changes on every stop/start, which would break DNS.
gcloud compute addresses create $ADDR --region=$REGION
IP=$(gcloud compute addresses describe $ADDR --region=$REGION --format='value(address)')

# Read the current access config name rather than assuming "External NAT".
AC=$(gcloud compute instances describe $VM --zone=$ZONE \
     --format='value(networkInterfaces[0].accessConfigs[0].name)')
gcloud compute instances delete-access-config $VM --zone=$ZONE --access-config-name="$AC"
gcloud compute instances add-access-config $VM --zone=$ZONE \
  --access-config-name="External NAT" --address=$IP

# --- Artifact Registry ---------------------------------------------
gcloud artifacts repositories create ferum \
  --repository-format=docker --location=$REGION

# --- Service account for CI ----------------------------------------
# compute.osLogin, NOT osAdminLogin. The Admin variant writes the account into
# /var/google-sudoers.d, i.e. passwordless root — at which point whether the
# workflow types `sudo` is cosmetic. What sudo this account gets is instead one
# pinned entry installed in Phase E.
gcloud iam service-accounts create gh-deploy
SA=gh-deploy@$PROJECT.iam.gserviceaccount.com
for R in artifactregistry.writer compute.osLogin iap.tunnelResourceAccessor; do
  gcloud projects add-iam-policy-binding $PROJECT \
    --member=serviceAccount:$SA --role=roles/$R
done

# --- OS Login ------------------------------------------------------
# Without this metadata, `gcloud compute ssh` falls back to writing the public
# key into instance SSH metadata and fails with "Required
# 'compute.instances.setMetadata' permission" — a permission not to grant, since
# metadata carries startup-script and so is root on the box by another route.
gcloud compute instances add-metadata $VM --zone=$ZONE \
  --metadata enable-oslogin=TRUE

# SSHing into a VM that runs as a service account requires permission to *use*
# that service account. Omit this and OS Login refuses with a
# 'compute.instances.osLogin' denial that does not mention the VM's own identity.
PNUM=$(gcloud projects describe $PROJECT --format='value(projectNumber)')
gcloud iam service-accounts add-iam-policy-binding \
  $PNUM-compute@developer.gserviceaccount.com \
  --member=serviceAccount:$SA --role=roles/iam.serviceAccountUser

# The POSIX name OS Login gives a service account is `sa_` + its numeric
# uniqueId. Phase E needs it verbatim, and it is knowable before first login.
echo "CI POSIX user = sa_$(gcloud iam service-accounts describe $SA --format='value(uniqueId)')"

# --- Workload Identity Federation ----------------------------------
# GitHub Actions mints a short-lived token per run. No service account
# key is ever created, so there is no long-lived secret to leak.
#
# A deleted pool keeps its name reserved for 30 days; recreating it then
# fails and needs `workload-identity-pools undelete`, not a new name.
gcloud iam workload-identity-pools create github --location=global

# The provider ID must be 4-32 chars of [a-z0-9-]. "gha" is rejected for
# being 3 characters, with an error that does not name length as the cause.
gcloud iam workload-identity-pools providers create-oidc github-actions \
  --location=global --workload-identity-pool=github \
  --issuer-uri=https://token.actions.githubusercontent.com \
  --attribute-mapping="google.subject=assertion.sub,attribute.repository=assertion.repository" \
  --attribute-condition="assertion.repository=='$REPO'"

POOL=$(gcloud iam workload-identity-pools describe github --location=global --format='value(name)')
gcloud iam service-accounts add-iam-policy-binding $SA \
  --role=roles/iam.workloadIdentityUser \
  --member="principalSet://iam.googleapis.com/$POOL/attribute.repository/$REPO"

# --- Let the VM pull images ----------------------------------------
# PNUM was read above.
gcloud projects add-iam-policy-binding $PROJECT \
  --member=serviceAccount:$PNUM-compute@developer.gserviceaccount.com \
  --role=roles/artifactregistry.reader

# --- Firewall ------------------------------------------------------
# SSH only from Google's IAP range; port 22 never faces the internet.
gcloud compute firewall-rules create allow-iap-ssh \
  --allow=tcp:22 --source-ranges=35.235.240.0/20

# HTTP/HTTPS only from Cloudflare, fetched live so the list cannot
# drift. Without this, anyone knowing $IP bypasses Cloudflare.
CF=$(curl -s https://www.cloudflare.com/ips-v4 | tr '\n' ',' | sed 's/,$//')
gcloud compute firewall-rules create allow-cloudflare \
  --allow=tcp:80,tcp:443 --target-tags=cf-origin --source-ranges=$CF

gcloud compute instances remove-tags $VM --zone=$ZONE --tags=http-server,https-server
gcloud compute instances add-tags $VM --zone=$ZONE --tags=cf-origin

# --- Output --------------------------------------------------------
echo "DNS A record  -> $IP"
echo "WIF_PROVIDER        = $(gcloud iam workload-identity-pools providers describe github-actions \
  --location=global --workload-identity-pool=github --format='value(name)')"
echo "WIF_SERVICE_ACCOUNT = $SA"
echo "CI POSIX user       = sa_$(gcloud iam service-accounts describe $SA --format='value(uniqueId)')"
gcloud compute instances describe $VM --zone=$ZONE --format='value(tags.items)'
```

The last four lines are the ones to keep. VM tags must read `cf-origin` and
nothing else.

---

## Phase B — GitHub secrets and variables

Repo → Settings → Secrets and variables → Actions. Two tabs, and which tab a
value goes in matters.

**Secrets** — masked everywhere in logs:

| Secret | Value |
| --- | --- |
| `WIF_PROVIDER` | the full `projects/…/providers/github-actions` path printed above |
| `WIF_SERVICE_ACCOUNT` | `gh-deploy@$PROJECT.iam.gserviceaccount.com` |
| `GCP_PROJECT` | `$PROJECT` |
| `VM_NAME` | `$VM` |

**Variables** — printed in logs, deliberately:

| Variable | Value | Default if unset |
| --- | --- | --- |
| `GCP_REGION` | `$REGION` | `us-central1` |
| `GCP_ZONE` | `$ZONE` | `us-central1-a` |
| `AR_REPOSITORY` | Artifact Registry repo name | `ferum` |
| `IMAGE_NAME` | image name within it | `ferum-board` |

The split is by what each value *is*. `GCP_PROJECT` and `VM_NAME` identify the
deployment, so on a public repository they are worth keeping out of build logs
even though neither is a credential — knowing a project ID grants nothing without
IAM. The rest are generic, and masking them would only make a typo harder to see.

**Masking costs something, and it is the right trade only if you meant it.** A
wrong `GCP_PROJECT` shows as `***` — including inside the registry path — so the
log cannot be read to find out what went wrong. Both jobs therefore begin with a
guard that fails on an unset secret and names it, because an empty value
otherwise reaches the registry as a malformed reference and produces an error
about references rather than about configuration.

**Secrets do not retract what is already published.** If earlier commits or
workflow runs carried these values on a public repository, they remain in the
git history and in the logs of past runs. Moving them into secrets narrows future
exposure only; undoing the rest means rewriting history and deleting those runs.

Nothing else. There is deliberately no service-account JSON key, no SSH private
key and no registry password in this repository.

---

## Phase C — Cloudflare

### DNS

DNS → Records → Add record:

| Type | Name | Content | Proxy |
| --- | --- | --- | --- |
| A | `@` | `$IP` | **Proxied** (orange) |
| A | `www` | `$IP` | **Proxied** (orange) |

**Proxied is not a preference — leaving these grey breaks the site in a way that
looks like the server is down.** Two independent mechanisms depend on it:

* The Phase A firewall admits ports 80/443 only from Cloudflare's ranges. Grey
  cloud sends the browser straight at the origin from a home IP, which is
  **dropped**, not rejected — so the browser hangs until it gives up with
  `ERR_CONNECTION_TIMED_OUT` and nothing is logged anywhere.
* The origin certificate is Cloudflare-signed and trusted by Cloudflare alone.
  Even with the firewall wide open, a direct browser connection fails on the
  certificate.

Add the records as **DNS only** if you like while the box is still being built,
but flip both to **Proxied** before expecting the site to answer. The MX records
stay DNS only — Cloudflare does not proxy mail.

Confirm from the outside once the stack is up: `curl -sI https://$DOMAIN/`
must show `server: cloudflare` and a `cf-ray:` header. Their absence means the
request never went through Cloudflare.

### TLS

1. **SSL/TLS → Overview → Full (strict).**
   Not *Flexible*: it speaks plain HTTP to the origin, so the app sees
   `X-Forwarded-Proto: http`, decides it is not on TLS, and drops the `Secure`
   attribute from the auth cookie.
2. **SSL/TLS → Origin Server → Create Certificate.** Defaults (RSA 2048, 15
   years) → Create. Copy both blocks; the private key is shown once.
3. **SSL/TLS → Edge Certificates → Always Use HTTPS: On.**

The Origin Certificate is not trusted by browsers, and that is the point —
nothing but Cloudflare should ever reach port 443 on this box. It also replaces
certbot: the ACME HTTP-01 challenge cannot complete while Cloudflare proxies
port 80.

---

## Phase D — Provision the VM

```bash
gcloud compute ssh $VM --zone=$ZONE --tunnel-through-iap
```

Then, inside the VM:

```bash
#!/bin/bash
set -e

sudo apt-get update && sudo apt-get -y upgrade

# --- Swap ----------------------------------------------------------
# 2 GB of insurance on a 4 GB box running five containers. Postgres is
# the process that must never be chosen by the OOM killer, and swap is
# what buys the kernel room to avoid choosing at all. It does not
# replace the memory limits in docker-compose.prod.yml.
sudo fallocate -l 2G /swapfile
sudo chmod 600 /swapfile
sudo mkswap /swapfile
sudo swapon /swapfile
echo '/swapfile none swap sw 0 0' | sudo tee -a /etc/fstab
echo 'vm.swappiness=10' | sudo tee /etc/sysctl.d/99-swappiness.conf
sudo sysctl -p /etc/sysctl.d/99-swappiness.conf

# --- Docker --------------------------------------------------------
sudo apt-get install -y ca-certificates curl
sudo install -m 0755 -d /etc/apt/keyrings
sudo curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc
sudo chmod a+r /etc/apt/keyrings/docker.asc

# Docker publishes one repo per Ubuntu codename and a new release can lag
# by weeks. 26.04 is "resolute"; fall back to noble (24.04) if it is not
# published yet — the packages are compatible.
. /etc/os-release
CODENAME=$VERSION_CODENAME
if ! curl -fsI "https://download.docker.com/linux/ubuntu/dists/$CODENAME/Release" >/dev/null 2>&1; then
  echo "No Docker repo for '$CODENAME' — using 'noble'"
  CODENAME=noble
fi

echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] \
https://download.docker.com/linux/ubuntu $CODENAME stable" \
  | sudo tee /etc/apt/sources.list.d/docker.list

sudo apt-get update
sudo apt-get install -y docker-ce docker-ce-cli containerd.io \
  docker-buildx-plugin docker-compose-plugin
sudo usermod -aG docker $USER

# --- Layout --------------------------------------------------------
sudo mkdir -p /opt/ferum/certs
sudo chown -R $USER:$USER /opt/ferum
chmod 700 /opt/ferum/certs

gcloud auth configure-docker $REGION-docker.pkg.dev --quiet

# The same, for root. `ferum-deploy` runs under sudo, so the pull authenticates
# as root and reads /root/.docker/config.json — a different file, which the line
# above does not write. Without this the deploy fails at `pull` with a
# `denied: Permission "artifactregistry.repositories.downloadArtifacts"` that
# reads like a missing IAM role rather than a missing credential helper. It
# authenticates as the VM's own service account, granted reader in Phase A.
sudo gcloud auth configure-docker $REGION-docker.pkg.dev --quiet
```

Log out and back in for the `docker` group to apply, then confirm with
`docker run --rm hello-world`.

> **Enabling OS Login in Phase A renames your own account, and `$USER` above is
> captured before that happens.** Without OS Login, `gcloud compute ssh` logs you
> in as the local part of your Google address — `jane` for `jane@example.com`;
> with it, as the whole address with punctuation folded to `_`, so
> `jane_example_com`. They are different Unix users, so after Phase A you are no
> longer in the `docker` group and no longer own `/opt/ferum` — `id -nG` and
> `ls -la /opt/ferum` disagreeing with each other is the tell.
>
> Nothing needs repairing: `/opt/ferum` becomes root-owned at the end of Phase E
> and `sudo docker` works regardless. But **every write into `/opt/ferum` from
> then on needs `sudo`**, so the `cat > …` forms below are written as `sudo tee`.
> Do Phase A before Phase D and the rename never happens mid-setup.

That group membership is for **you**, not for CI. Adding the deploy account to
`docker` instead of giving it the pinned sudo entry below would not be a
reduction in privilege: any member of the `docker` group can start a container
that bind-mounts `/` with `--privileged`, so the group is root-equivalent by
design — the same access, minus the audit trail.

---

## Phase E — Configuration on the box

### Certificates

```bash
sudo tee /opt/ferum/certs/cloudflare-origin.pem >/dev/null   # paste, then Ctrl-D
sudo tee /opt/ferum/certs/cloudflare-origin.key >/dev/null   # paste, then Ctrl-D
sudo chmod 600 /opt/ferum/certs/cloudflare-origin.key
```

### `.env.prod`

`sudo tee`, and the heredoc delimiter is deliberately **unquoted** — the opposite
of the one used to write `ferum-deploy`. The `$(openssl rand …)` calls must run in
your shell so real secrets are generated; quoting `EOF` would write the literal
text `$(openssl rand -hex 32)` into the file, and Postgres would start with that
as its password.

> **First install only.** `tee` truncates. Re-running this block on a box that is
> already serving regenerates `JWT_SECRET` and `DB_PASSWORD`, at which point the app
> can no longer authenticate to its own database and every existing session is
> invalidated. To add a variable to a running deployment — `SECRET_ENCRYPTION_KEY`,
> `RESEND_API_KEY` — append it instead, then recreate the container:
>
> ```bash
> cd /opt/ferum
> echo "SECRET_ENCRYPTION_KEY=$(openssl rand -hex 32)" | sudo tee -a .env.prod
> sudo docker compose --env-file .env.prod -f docker-compose.prod.yml up -d --force-recreate app
> ```
>
> `--force-recreate`, not `restart`: `env_file` is read when a container is
> **created**, so a restart reuses the old environment and the new variable appears
> to have no effect.

```bash
sudo tee /opt/ferum/.env.prod >/dev/null <<EOF
# The registry path, without a tag. docker-compose.prod.yml reads it from here
# rather than hardcoding it, so that file is identical for every installation.
APP_IMAGE=$REGION-docker.pkg.dev/$PROJECT/ferum/ferum-board

APP_URL=https://$DOMAIN
CORS_ORIGINS=https://$DOMAIN,https://www.$DOMAIN
JWT_SECRET=$(openssl rand -hex 32)
JWT_EXPIRY_SECONDS=3600
REFRESH_TOKEN_EXPIRY_DAYS=7
FROM_EMAIL=noreply@$DOMAIN

DB_PASSWORD=$(openssl rand -hex 24)
DB_MAX_CONNECTIONS=16
DB_MIN_CONNECTIONS=2

S3_ACCESS_KEY=$(openssl rand -hex 16)
S3_SECRET_KEY=$(openssl rand -hex 32)
S3_BUCKET=forum-uploads

SECRET_ENCRYPTION_KEY=$(openssl rand -hex 32)

SMTP_HOST=smtp.sendgrid.net
SMTP_PORT=587
SMTP_USER=apikey
SMTP_PASS=REPLACE_ME
EOF
sudo chmod 600 /opt/ferum/.env.prod
```

`DB_MAX_CONNECTIONS=16` rather than the default 40. Handlers run their COUNT and
data fetch concurrently, so one request holds 4–6 connections; 16 covers the
~12 req/s this box sustains, and each idle Postgres backend still costs memory
against a 1 GB container limit.

**`FROM_EMAIL` carries the sender name too.** There is no separate variable for it:
`FROM_EMAIL=Ferum Board <noreply@$DOMAIN>` works, and applies to both providers. The
value is parsed at startup and a malformed one **aborts the boot** — deliberately,
because it used to be parsed only at send time, which meant a typo produced a
container that came up healthy and then failed every message with a generic
`internal_error`. Under Resend, make the address one on a domain you have verified
there; an unverified sending domain is the most common rejection.

**Mail is either SMTP or Resend, and Resend wins.** The four `SMTP_*` variables
above seed `site_config` on first start, after which `/admin/settings` → Email is
the authority and applies changes to the live transport with no restart. To use
Resend's API instead, drop the `SMTP_*` block and set `RESEND_API_KEY=re_…`; it is
env-only by design, so rotating it needs a restart. Setting both logs a warning
naming SMTP as the ignored one.

TLS is inferred from the address, with nothing to configure: loopback connects in
the clear, port 465 uses implicit TLS, every other host requires STARTTLS. A relay
that does not offer STARTTLS is now refused rather than sent credentials in the
clear — if you are migrating from an older build that appeared to work against
such a relay, check delivery with the **Send test email** button on that settings
tab before announcing anything.

**With no mail provider at all, every new registration is auto-verified** —
otherwise nobody could ever complete signup. That is the right trade on a laptop
and a real problem in public: anyone can register with an address they do not own,
and password reset cannot work. Startup logs a warning when `APP_URL` is https and
no provider is configured, and `/health/ready` reports
`"mail": "resend" | "smtp" | "disabled"`.

`--env-file` is not optional when running compose by hand. `env_file:` supplies
variables *inside* a container; the `${...}` substitutions in the compose file
are resolved by Compose itself, which reads only `--env-file`. Omit it and
`DB_PASSWORD` expands to empty, and Postgres refuses to initialise.

### Files from the repository

From a local checkout:

Everything the VM needs at run time lives in `deploy/`. From a local checkout:

**Pull them from GitHub, on the VM.** One command per file, nothing to upload and
no second window:

```bash
curl -fsS -o /tmp/dc.yml         https://raw.githubusercontent.com/$REPO/develop/deploy/docker-compose.prod.yml
curl -fsS -o /tmp/nginx.conf     https://raw.githubusercontent.com/$REPO/develop/deploy/nginx.conf
curl -fsS -o /tmp/ferum-deploy   https://raw.githubusercontent.com/$REPO/develop/deploy/ferum-deploy
wc -l /tmp/dc.yml /tmp/nginx.conf /tmp/ferum-deploy
sudo mv /tmp/dc.yml     /opt/ferum/docker-compose.prod.yml
sudo mv /tmp/nginx.conf /opt/ferum/nginx.conf
```

This is also how you update them later, which is the case that recurs —
`docker-compose.prod.yml` and `nginx.conf` are **not** shipped by the deploy
workflow, only the image is. Re-pull whenever either changes in git.

> Two other routes exist and both cost more than they look. `gcloud compute scp`
> runs **only in Cloud Shell** (on the VM it fails on scopes) and needs the files
> to be in Cloud Shell first. The SSH-in-browser **UPLOAD FILE** button opens a
> pop-up, which Firefox blocks by default — and a blocked pop-up produces no
> error anywhere, just a home directory that stays empty. If `curl` 404s because
> the repository is private, use `scp` from Cloud Shell rather than fighting the
> pop-up blocker.

### What CI is allowed to run

Still on the VM — `hostname` must read `$VM`. Cloud Shell
also grants sudo, so every command below "succeeds" there while changing nothing
that matters.

The deploy account has no sudo of its own (Phase A grants `compute.osLogin`, not
`osAdminLogin`). One script, and only that script, is what it may run as root.

```bash
sudo install -o root -g root -m 755 /tmp/ferum-deploy /usr/local/sbin/ferum-deploy

# The value Phase A printed as "CI POSIX user". Set it as a variable rather than
# editing the line below: a placeholder pasted through by mistake is a sudoers
# syntax error, and sudo then warns on EVERY invocation until the file is removed.
# `${CI_USER:?}` makes that mistake impossible — the block stops instead.
CI_USER=sa_000000000000000000000        # ← replace, or export it beforehand
: "${CI_USER:?set CI_USER to the sa_<uniqueId> Phase A printed}"

printf '%s\n' "$CI_USER ALL=(root) NOPASSWD: /usr/local/sbin/ferum-deploy" \
  > /tmp/ferum-sudoers
sudo visudo -c -f /tmp/ferum-sudoers    # must report "parsed OK"
sudo install -o root -g root -m 440 /tmp/ferum-sudoers /etc/sudoers.d/ferum-deploy
rm /tmp/ferum-sudoers
```

`ferum-deploy` pulls, restarts `app`, waits up to 150s for `/health/ready`, and
prints the last 50 log lines if it never answers. The readiness wait was a second
workflow step with a second sudo-able script behind it; folding it in leaves one
entry point to audit and one ssh session per deploy.

Validate in `/tmp` and install only on success — never write into
`/etc/sudoers.d/` and check afterwards. A malformed file there is read by every
`sudo` on the box; this sudo skips it with a warning, but a stricter build
refuses outright, and then nothing can be fixed because fixing it needs `sudo`.
The box has no other way in.

### Preflight — check all of it at once

Run this on the VM before pushing. Everything the `release` job needs, reported
in one go; it changes nothing.

```bash
if [[ "$(hostname)" != $VM ]]; then
  echo "STOP — this is $(hostname), not the VM."
  echo "Run: gcloud compute ssh $VM --zone=$ZONE --tunnel-through-iap"
else
  echo "=== /opt/ferum ==="       ; ls -la /opt/ferum 2>&1
  echo "=== script ==="           ; ls -l /usr/local/sbin/ferum-deploy 2>&1
  echo "=== sudoers ==="          ; sudo cat /etc/sudoers.d/ferum-deploy 2>&1
  echo "=== what CI may run ===" ; sudo -l -U "${CI_USER:?set CI_USER}" 2>&1
  echo "=== root docker auth ===" ; sudo cat /root/.docker/config.json 2>&1
  echo "=== daemon ==="           ; sudo docker ps --format '{{.Names}}' 2>&1
fi
```

**The hostname guard is the point of the block, not decoration.** Cloud Shell and
the VM are both Linux, both have `sudo`, and both have `gcloud` — nothing in the
prompt distinguishes them. Phase E was run against Cloud Shell three times before
this guard existed, and every symptom (`No such file or directory`, `unknown
user`) was a truthful description of Cloud Shell rather than a clue about the VM.
Cloud Shell hostnames start `cs-`; the VM answers with its instance name.

There is a second, involuntary tell. Any `gcloud` call that reaches an API —
`compute scp`, `compute ssh`, `services enable` — fails on the VM with
`Request had insufficient authentication scopes`, because gcloud there
authenticates as the VM's own service account under default access scopes.
Cloud Shell never produces that error. So a scope failure means you are on the
VM, and the local commands (`install`, `visudo`, `gcloud auth configure-docker`)
that make up the rest of Phase E will all work exactly where you already are.

`sudo -l -U <user>` is the decisive line: it reports what sudo believes that
account may run, without becoming it. It must list
`/usr/local/sbin/ferum-deploy`.

Then prove the whole path without CI at all — the image for any previously built
commit is already in Artifact Registry:

```bash
sudo /usr/local/sbin/ferum-deploy <sha-of-a-build-that-pushed>
```

Every VM-side failure so far was found one per pipeline run, at ~40 minutes each,
because it can only surface after the tests and the image build. This block finds
them all in a few seconds. Use it before pushing, not after.

### Lock the working directory

```bash
sudo chown -R root:root /opt/ferum
sudo chmod 600 /opt/ferum/.env.prod
sudo chmod 700 /opt/ferum/certs
```

Do this last — Phase E's earlier steps are easier with the directory owned by
you, and from here on editing `.env.prod` needs `sudo`.

It is also what makes the script above worth anything. `ferum-deploy` pins the
compose file, but an account that can *edit* `docker-compose.prod.yml` can add
`- /:/host` to it and be root on the next deploy. The allowlist and the file
ownership are one mechanism, not two.

### Bring the stack up once, by hand

Do this before involving CI. It proves every file above is in place, and it takes
a minute instead of a pipeline.

```bash
sudo /usr/local/sbin/ferum-deploy <sha-of-any-image-already-in-Artifact-Registry>
sudo docker ps --format '{{.Names}}\t{{.Status}}'
```

All five containers must appear — `app`, `postgres`, `redis`, `minio`, **and
`nginx`** — with `app` reporting `(healthy)`. A missing `nginx` means the site
will time out on 443 no matter how green the pipeline is; that happens with an
older `ferum-deploy` that ran `up -d app` rather than a bare `up -d`, because
`nginx depends_on app` and not the reverse, so Compose never pulled it in.

Then, still from the box, working outward:

```bash
curl -I  http://localhost/                 # 301 → https
curl -kI https://localhost/health/ready    # 200 — nginx and the app agree
curl -I  https://$DOMAIN/health/ready  # 200 — DNS, firewall and Cloudflare agree
```

The three lines separate the three layers. The last one failing while the first
two pass is a Cloudflare or firewall problem, never an application one.

---

## Phase F — First deploy

```bash
git add .github/workflows/ci.yml Dockerfile .dockerignore docs/deployment.md
git add --chmod=+x deploy/ferum-deploy
git add deploy/
git commit -m "Add production deployment"
git push origin develop
```

`--chmod=+x` because git records mode 644 for a file created on Windows. The VM
gets its mode from `install -m 755` either way, but a clone on Linux would
otherwise hold a deploy script that will not run.

The tests run first (~15 min: the matrix runs twice, under `UTC` and
`Asia/Kathmandu`). `image` starts only once they and the lint job are green; the
first build is ~27 min and later ones ~15, because cargo-chef puts the dependency
crates in a layer keyed on `Cargo.toml`/`Cargo.lock` and the registry build cache
carries it between runs. `release` follows in about a minute.

Watch it land:

```bash
cd /opt/ferum
sudo docker compose --env-file .env.prod -f docker-compose.prod.yml logs -f
```

`sudo` because Phase E moved the directory to root. Your own account is in the
`docker` group, so the daemon is reachable either way — it is `.env.prod` at
mode 600 that is not.

On the very first boot the app runs migrations and `PgSystemSeedService`, then
`setup_guard` redirects every HTML request to `/setup`. Open
`https://$DOMAIN` and create the admin account.

---

## Phase G — Verify

```bash
curl -I https://$DOMAIN/                 # 200, X-Cache-Status: MISS then HIT
curl -I https://$DOMAIN/health/ready     # 200
curl -sI https://$DOMAIN/ | grep -i strict-transport   # HSTS present
```

| Symptom | Cause |
| --- | --- |
| `X-Cache-Status: BYPASS` while logged out | the `map $http_cookie` block in `nginx.conf` |
| HSTS header missing | an `add_header` was added to a `location` without repeating HSTS — nginx applies outer `add_header` only when the inner level declares none |
| `ERR_CONNECTION_TIMED_OUT`, nothing in any log | the A records are grey-cloud (**DNS only**), so the browser bypasses Cloudflare and the origin firewall drops it. Flip both A records to **Proxied** |
| Cloudflare **521** | origin unreachable: container down, or the firewall tag is not `cf-origin` |
| Cloudflare **526** | SSL mode is not Full (strict), or the origin certificate does not match |
| Every user hits the rate limit at once | `X-Forwarded-For` is being appended rather than overwritten — see the note in `nginx.conf` |

Confirm the real client IP is reaching the app by logging in with a wrong
password six times: the lockout must apply to that account, not to everyone.

---

## When the deploy fails

CI splits this into two jobs. `image` builds and pushes; `release` rolls the VM.
A `release` failure is retried with **Re-run failed jobs**, which re-runs only
that job — about a minute, against the image `image` already published. Only a
genuine build failure costs a rebuild.

Every failure below was hit for real on the way to the first green deploy, and
they share a shape worth naming: **the message describes the symptom's API, never
the setup step that was skipped.** `docker push` reported a credential error for a
disabled IAM API; `gcloud compute ssh` reported a metadata permission for OS Login
being off; a readiness loop reported the app unhealthy while the app was serving.
So read the table by symptom, and do not trust the error's own account of its
cause.

The cheapest way to avoid all of them is the Phase E preflight plus one manual
`ferum-deploy` on the box. Both run in seconds; each round through CI costs ~15
minutes to reach the same information.

| Message | Cause | Fix |
| --- | --- | --- |
| `error getting credentials - err: exit status 1, out: ``` at `docker push` | `iamcredentials.googleapis.com` disabled, so impersonating `gh-deploy@` yields no token. `auth@v2` only writes a credential *file*, so the auth step passes and this surfaces later, at the first real use | `gcloud services enable iamcredentials.googleapis.com` |
| `Required 'compute.instances.setMetadata' permission` | OS Login is off, so `gcloud compute ssh` fell back to writing the key into instance metadata | `gcloud compute instances add-metadata $VM --metadata enable-oslogin=TRUE` |
| `Required 'compute.instances.osLogin' permission`, or a `serviceAccountUser` denial | the VM runs as a service account and the caller may not use it | bind `roles/iam.serviceAccountUser` on the VM's SA (Phase A) |
| `sudo: a password is required`, or `sudo: I'm sorry <user>. I'm afraid I can't do that` | `/etc/sudoers.d/ferum-deploy` missing, or names the wrong `sa_<uniqueId>`. The second wording is sudo's easter egg for "no sudoers entry at all" and means the same thing | run the Phase E preflight; `sudo -l -U sa_…` states it directly |
| `denied: Permission "artifactregistry.repositories.downloadArtifacts"` at `pull` | root has no credential helper — `gcloud auth configure-docker` was run as your user only | `sudo gcloud auth configure-docker $REGION-docker.pkg.dev --quiet` |
| `ferum-deploy: refusing image tag` | something passed the script a value that is not a 40-hex SHA | check the `--command=` line in the workflow; the guard is working |
| `ferum-deploy` reports ready, but the site times out on 443 | nginx is not running. An older `ferum-deploy` ran `up -d app`, and `nginx depends_on app` — not the reverse — so Compose never started it | reinstall the current `deploy/ferum-deploy`, which runs a bare `up -d`; or once by hand: `cd /opt/ferum && sudo docker compose --env-file .env.prod -f docker-compose.prod.yml up -d` |
| `did not become ready within 150s`, but the dumped log shows the app answering `/health/ready` 200 every 30s | the readiness probe was curling `localhost:5173` **on the host**. `app` publishes no ports — that address belongs to nothing. The 200s in the log are the container probing itself | reinstall the current `deploy/ferum-deploy`, which runs the probe with `compose exec -T app` |
| `/etc/sudoers.d/ferum-deploy:1:16: syntax error` on every `sudo` | the `sa_<uniqueId>` placeholder was installed literally | `sudo rm /etc/sudoers.d/ferum-deploy`, then redo Phase E validating in `/tmp` first |
| `install: cannot stat '/tmp/ferum-deploy'` together with `chown: cannot access '/opt/ferum'` | Phase E is being run in Cloud Shell rather than on the VM | check `hostname` |
| `Request had insufficient authentication scopes` from any `gcloud` | the reverse of the row above — a Cloud Shell command run on the VM, where gcloud is the VM's service account under default scopes | `exit` back to Cloud Shell. For instance metadata specifically, the metadata server needs no scopes at all: `curl -s -H "Metadata-Flavor: Google" http://metadata.google.internal/computeMetadata/v1/instance/tags` |
| **UPLOAD FILE** in SSH-in-browser does nothing, home directory stays empty | the browser blocked the pop-up the button opens; nothing reports this | allow pop-ups for the site, or skip it — Phase E pulls the files with `curl` |
| `id -nG` does not list `docker`, and `/opt/ferum` is owned by a username unlike your own | Phase A's OS Login was enabled after Phase D, renaming your account | nothing to repair; use `sudo` for writes into `/opt/ferum`, which ends up root-owned anyway |

Empty `out: ``` means no token was obtained at all — a WIF or impersonation
problem. An HTTP status means the token was fine and a role is missing on the
resource. That one distinction separates the top two rows from everything else.

---

## Operations

### Rollback

Images are tagged by commit SHA and the tag is immutable, so rolling back is
pointing the VM at an older one. On the box:

```bash
sudo /usr/local/sbin/ferum-deploy <sha>
```

The same script CI runs, so a rollback exercises the path that is exercised on
every deploy rather than a hand-typed variant of it.

That is the fast path — seconds, no rebuild. There is deliberately no
**Run workflow** button: `workflow_dispatch` needs the workflow file on the
default branch, the constraint this pipeline is built to avoid. To redeploy an
older commit *through* CI instead, open its run under Actions → CI and re-run it
— the re-run keeps the original SHA, so `release` ships that commit. Prefer
**Re-run failed jobs** where something failed: it leaves the already-published
image alone. **Re-run all jobs** repeats the tests and the build too.

### Logs

```bash
cd /opt/ferum
sudo docker compose --env-file .env.prod -f docker-compose.prod.yml logs -f app
sudo docker compose --env-file .env.prod -f docker-compose.prod.yml logs --tail=100 postgres
docker stats            # which container is actually eating the box
```

### Backups

Disk snapshots are scheduled (`default-schedule-1`) but a snapshot of a running
Postgres is *crash*-consistent, not application-consistent — restoring one
replays WAL as though after a power cut. Usually fine; not something to bet the
forum on. Add a logical dump:

```bash
# /etc/cron.daily/ferum-backup   — set BACKUP_BUCKET to a bucket you created
docker compose -f /opt/ferum/docker-compose.prod.yml exec -T postgres \
  pg_dump -U forum -Fc forum | gzip > /tmp/f.dump.gz
gcloud storage cp /tmp/f.dump.gz gs://$BACKUP_BUCKET/$(date -u +%F).dump.gz
rm /tmp/f.dump.gz
```

`-Fc` allows selective restore of individual tables. That backup bucket must
**not** be public, unlike the uploads bucket.

### Secrets at rest

`SECRET_ENCRYPTION_KEY` encrypts every secret this application stores in
PostgreSQL: the SMTP password in `site_config`, each webhook's HMAC key, and every
plugin config field whose manifest marks it `secret = true` (the Discord notifier's
webhook URL is one — holding it is enough to post to the channel). It is
XChaCha20-Poly1305 with a random nonce per value, and each ciphertext is bound to
the row it belongs to, so one moved between columns — or between plugins — fails
authentication rather than decrypting.

**It is mandatory on any https deployment: the app refuses to start without it.**
That check does not wait until there is a secret to protect. It fires on an empty
database, at first deploy, because the alternative is a boot that succeeds right up
until the operator saves SMTP settings — the worst possible moment to discover the
requirement. On an http `APP_URL` (a laptop) it stays a warning.

It exists for the case the section above creates: **a `pg_dump` sitting in a
bucket.** It does not protect against an attacker on the box, where the key is in
the process environment — nobody should deploy it expecting otherwise.

**Which means the key must not live beside the backups.** Storing it in the same
bucket as `ferum-backup` writes to reduces this to obfuscation. It belongs wherever
`JWT_SECRET` is kept — a password manager, or Secret Manager — and it must be
recorded *somewhere*, because `.env.prod` on a single VM is not a backup of it.

**Enabling it on a running forum needs no migration.** Plaintext values are read
unchanged, and the first start with the key set converts them in one transaction
and logs the count. The admin Email tab reports which state you are in.

**But it does nothing for backups already taken.** The sweep updates rows in place,
so the old plaintext tuples survive until `VACUUM` and every earlier `pg_dump` is
untouched. If a pre-encryption backup may have leaked, rotate the SMTP password,
every webhook secret and every plugin credential — encrypting today's values does
not help copies that already left.

**Startup refuses to boot** if the key does not match the data, or if it is missing
while sealed values exist. That is deliberate: continuing would leave the site
unable to read its own secrets, and re-saving any of them would seal them under the
wrong key and destroy the originals. A refused boot is recoverable.

**Rotation** — four steps, no downtime beyond two container restarts.

`ferum-deploy` takes a commit SHA and nothing else (that is what makes it safe to
expose through sudo), so a config-only restart is a plain compose command run as
root on the box:

```bash
cd /opt/ferum
alias fc='sudo docker compose --env-file .env.prod -f docker-compose.prod.yml'

# 1. Retire the old key, install a new one.
sudo sed -i 's/^SECRET_ENCRYPTION_KEY=/SECRET_ENCRYPTION_KEY_PREVIOUS=/' .env.prod
echo "SECRET_ENCRYPTION_KEY=$(openssl rand -hex 32)" | sudo tee -a .env.prod

# 2. Recreate `app` so it reads the new env. `restart` would NOT do — it reuses
#    the existing container, and env_file is read at create time.
fc up -d --force-recreate app

# 3. Confirm — look for `sealed secrets at rest` with a non-zero count.
fc logs app | grep 'sealed secrets'

# 4. Drop the retired key and recreate once more.
sudo sed -i '/^SECRET_ENCRYPTION_KEY_PREVIOUS=/d' .env.prod
fc up -d --force-recreate app
```

The same `--force-recreate` applies to *any* change to `.env.prod`, including
adding `SECRET_ENCRYPTION_KEY` or `RESEND_API_KEY` for the first time.

**If the key is lost**, those two values are unrecoverable — that is what
encryption at rest means. **Nothing else in the database is encrypted**, so posts,
users, threads and uploads are untouched. Clear the sealed values and re-enter
them:

```sql
UPDATE site_config SET value = ''   WHERE key = 'smtp_pass' AND value LIKE 'enc:v1:%';
UPDATE webhooks    SET secret = NULL WHERE secret LIKE 'enc:v1:%';
```

Then set a fresh `SECRET_ENCRYPTION_KEY` (or unset it), restart, and re-enter the
SMTP password in `/admin/settings` and each webhook secret in its own form.

### Cost

| Item | USD/month |
| --- | --- |
| e2-medium | 24.46 |
| 30 GB pd-balanced | 3.00 |
| External IPv4 | ~2.90 |
| Snapshots | 0.50–1.50 |
| Egress | near zero behind Cloudflare |
| **Total** | **~$32** |

A 1-year committed use discount takes the VM to roughly $15, so ~$23 total.
Compute Engine sustained-use discounts do **not** apply to E2.

---

## Known limitations

**Every deploy is a full recompile (~20–30 min).** The Dockerfile does
`COPY backend ./backend` and then `cargo build`, so any source change invalidates
the whole build layer; `cache-from: type=gha` only helps the apt and runtime
stages. The fix is `cargo-chef`, which splits the dependency build into its own
cacheable layer. Even then `lto = "fat"` + `codegen-units = 1` costs ~10–15
minutes in the final link, and `[profile.release-fast]` must not be shipped —
CLAUDE.md rules that out explicitly.

**Cloudflare IP ranges are pinned in two places** — the `allow-cloudflare`
firewall rule and the `set_real_ip_from` list in `nginx.conf`. A range added
upstream fails quietly: those visitors get a 503, or land in one shared
rate-limit bucket. Re-check `https://www.cloudflare.com/ips/` a few times a year.

**Published attachments are permanently world-readable.** Promotion into the
object store is one-way; deleting a post makes `/files/` return 404 and nothing
more. The per-account upload quota bounds the residue. See the storage section
of CLAUDE.md for why that is the deliberate position.

**Serving from `$REGION` costs Vietnamese readers ~200 ms.** Cloudflare's
edge absorbs this for cached guest pages, but not for anything dynamic. Moving to
`asia-southeast1` requires a new VM — the zone cannot be changed in place.

**This box holds all data on its boot disk.** Docker volumes live in
`/var/lib/docker/volumes`, so the VM's deletion rule is set to **Keep disk** and
deletion protection is enabled. Neither is a backup.
