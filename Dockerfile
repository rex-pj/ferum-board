# Ferum Board — production image.
#
# Build context is the REPOSITORY ROOT, not backend/: the runtime image needs
# frontend/ and locales/ as well as the binary.
#
#   docker build -t ferum-board .
#   docker build -t ferum-board --build-arg FEATURES=gcs .

# ─── Stage 1: build ───────────────────────────────────────────────────────────
#
# chef/planner/builder split so the ~400 dependency crates are their own layer,
# keyed on Cargo.toml/Cargo.lock alone. A single stage recompiled everything on
# any source edit: ~27 minutes.
#
# ONLY PAYS OFF IF THE LAYER CACHE SURVIVES BETWEEN RUNS — that is the
# cache-from/cache-to in ci.yml. Remove those and it is 27 minutes again.
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

# Storage features are opt-in; without the flag the S3_*/GCS_*/R2_* variables are
# ignored and uploads silently land in Postgres. `s3` by default because
# docker-compose.prod.yml ships MinIO. Use gcs / r2 / "" as appropriate, and add
# `,meilisearch` when running one.
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

# Pristine built-in themes, kept OUTSIDE /app/frontend/themes.
#
# Docker seeds a named volume from the image only when the volume is first
# created, so after the first deploy the volume shadows the image and
# themes/default/ freezes while static/ keeps updating. docker-entrypoint.sh
# copies this back over the volume on every start.
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
