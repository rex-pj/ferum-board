# Ferum Board — production image.
#
# Build context is the REPOSITORY ROOT, not backend/: the runtime image needs
# frontend/ and locales/ as well as the binary.
#
#   docker build -t ferum-board .
#   docker build -t ferum-board --build-arg FEATURES=gcs .

# ─── Stage 1: build ───────────────────────────────────────────────────────────
#
# Split into chef/planner/builder so that COMPILING THE 393 DEPENDENCY CRATES IS
# ITS OWN LAYER. The single-stage version copied all of backend/ and ran one
# `cargo build`, so editing one line of one handler invalidated the layer and
# recompiled every dependency from scratch — ~27 minutes on a GitHub runner, of
# which roughly 12 was work already done on the previous run.
#
# cargo-chef gets there by reducing the workspace to a "recipe": the manifests
# and a stub for every crate's source. `cook` builds the dependency graph from
# that recipe alone, so the layer's cache key changes only when Cargo.toml or
# Cargo.lock does. Real sources arrive afterwards.
#
# This only pays off if the layer cache SURVIVES BETWEEN RUNS, which on hosted
# runners means the `cache-from`/`cache-to` in .github/workflows/ci.yml. Removing
# those puts the build straight back to 27 minutes with extra stages to read.
#
# The floor is about 15 minutes regardless: the `release` profile uses fat LTO in
# one codegen unit, and the final unit alone measured 859s. That is a deliberate
# trade (see CLAUDE.md, "Build profiles") and not something to fix here.
FROM rust:1-slim-bookworm AS chef

# clang + mold are a hard requirement, not an optimisation: backend/.cargo/config.toml
# pins the Linux linker to clang with -fuse-ld=mold, so the link step fails outright
# without them. build-essential is for jemalloc, which is built from C source.
RUN apt-get update && apt-get install -y --no-install-recommends \
        clang \
        mold \
        build-essential \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

RUN cargo install cargo-chef --locked

# The whole build happens inside backend/ rather than at /src with
# --manifest-path, because `cargo chef prepare` reads the workspace from the
# working directory.
WORKDIR /src/backend

FROM chef AS planner
COPY backend .
RUN cargo chef prepare --recipe-path /recipe.json

FROM chef AS builder

# Cargo features are opt-in for anything needing external infrastructure. `s3` is
# the default here because deploy/docker-compose.prod.yml ships MinIO; without the
# feature the S3_* variables are ignored and uploads silently land in Postgres.
# Use `gcs` for Google Cloud Storage, `r2` for Cloudflare R2, or an empty string
# for database storage. Add `,meilisearch` when running a Meilisearch instance.
ARG FEATURES=s3

# .cargo/config.toml MUST be in place before `cook`. It sets `rustflags` for the
# Linux target, and rustflags are part of every crate's fingerprint — cooking
# without it produces artifacts the real build considers stale and recompiles,
# which looks exactly like cargo-chef not working at all.
COPY backend/.cargo ./.cargo
COPY --from=planner /recipe.json /recipe.json

# Same profile and same feature set as the build below, for the same reason: a
# mismatch on either silently invalidates everything cooked here.
RUN if [ -n "$FEATURES" ]; then \
        cargo chef cook --release -p ferum-web --features "$FEATURES" --recipe-path /recipe.json; \
    else \
        cargo chef cook --release -p ferum-web --recipe-path /recipe.json; \
    fi

COPY backend .

RUN if [ -n "$FEATURES" ]; then \
        cargo build --release -p ferum-web --features "$FEATURES"; \
    else \
        cargo build --release -p ferum-web; \
    fi

# ─── Stage 2: runtime ─────────────────────────────────────────────────────────
FROM debian:bookworm-slim

# ca-certificates: outbound TLS to SMTP, S3/GCS and plugin webhooks.
# curl: the container healthcheck in deploy/docker-compose.prod.yml.
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --uid 10001 ferum

WORKDIR /app

COPY --from=builder /src/backend/target/release/ferum-board /usr/local/bin/ferum-board
COPY frontend ./frontend
COPY locales  ./locales

# A pristine copy of the built-in themes, kept OUTSIDE /app/frontend/themes.
#
# That directory carries a named volume in production so admin-uploaded themes
# survive a container replacement — but Docker seeds a named volume from the
# image only once, when the volume is first created. Every deploy after that,
# the volume shadows the image, and frontend/themes/default/ (build output, not
# user data) freezes at whatever version created the volume while static/ and
# templates/ keep updating. docker-entrypoint.sh copies this back over the
# volume on every start so the two halves cannot drift apart.
RUN cp -a /app/frontend/themes /app/builtin-themes

# At the repository root, not under deploy/: .dockerignore excludes deploy/
# because it describes how the image is RUN, and is never consulted while
# building one. This script is part of the image, so it lives beside the
# Dockerfile that copies it.
COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-entrypoint.sh

# Uploaded plugins are extracted here at runtime. Mount a volume over it in
# production or installed plugins are lost when the container is replaced.
RUN mkdir -p /app/plugins \
    && chown -R ferum:ferum /app/plugins /app/frontend/themes /app/builtin-themes

# These are resolved relative to the process working directory, so they are set
# absolutely here. Getting one wrong is a silent failure: missing templates render
# a blank page, a missing LOCALES_DIR aborts startup.
ENV THEMES_DIR=/app/frontend/themes \
    ADMIN_TEMPLATES_DIR=/app/frontend/templates \
    STATIC_DIR=/app/frontend/static \
    LOCALES_DIR=/app/locales \
    PLUGINS_DIR=/app/plugins \
    BUILTIN_THEMES_DIR=/app/builtin-themes \
    BIND_ADDR=0.0.0.0 \
    PORT=5173 \
    LOG_FORMAT=json

USER ferum
EXPOSE 5173

# Migrations run inside the process at startup, so there is no separate migrate
# step. SIGTERM triggers a graceful drain, which is what `docker stop` sends —
# the entrypoint `exec`s the server so that signal reaches it rather than a shell.
ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
CMD ["ferum-board"]
