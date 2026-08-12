# Ferum Board — production image.
#
# Build context is the REPOSITORY ROOT, not backend/: the runtime image needs
# frontend/ and locales/ as well as the binary.
#
#   docker build -t ferum-board .
#   docker build -t ferum-board --build-arg FEATURES=gcs .

# ─── Stage 1: build ───────────────────────────────────────────────────────────
FROM rust:1-slim-bookworm AS builder

# clang + mold are a hard requirement, not an optimisation: backend/.cargo/config.toml
# pins the Linux linker to clang with -fuse-ld=mold, so the link step fails outright
# without them. build-essential is for jemalloc, which is built from C source.
RUN apt-get update && apt-get install -y --no-install-recommends \
        clang \
        mold \
        build-essential \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src

# Cargo features are opt-in for anything needing external infrastructure. `s3` is
# the default here because docker-compose.prod.yml ships MinIO; without the
# feature the S3_* variables are ignored and uploads silently land in Postgres.
# Use `gcs` for Google Cloud Storage, or an empty string for database storage.
# Add `,meilisearch` when running a Meilisearch instance.
ARG FEATURES=s3

COPY backend ./backend

RUN if [ -n "$FEATURES" ]; then \
        cargo build --release --manifest-path backend/Cargo.toml -p ferum-web --features "$FEATURES"; \
    else \
        cargo build --release --manifest-path backend/Cargo.toml -p ferum-web; \
    fi

# ─── Stage 2: runtime ─────────────────────────────────────────────────────────
FROM debian:bookworm-slim

# ca-certificates: outbound TLS to SMTP, S3/GCS and plugin webhooks.
# curl: the container healthcheck in docker-compose.prod.yml.
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --uid 10001 ferum

WORKDIR /app

COPY --from=builder /src/backend/target/release/ferum-board /usr/local/bin/ferum-board
COPY frontend ./frontend
COPY locales  ./locales

# Uploaded plugins are extracted here at runtime. Mount a volume over it in
# production or installed plugins are lost when the container is replaced.
RUN mkdir -p /app/plugins && chown -R ferum:ferum /app/plugins /app/frontend/themes

# These are resolved relative to the process working directory, so they are set
# absolutely here. Getting one wrong is a silent failure: missing templates render
# a blank page, a missing LOCALES_DIR aborts startup.
ENV THEMES_DIR=/app/frontend/themes \
    ADMIN_TEMPLATES_DIR=/app/frontend/templates \
    STATIC_DIR=/app/frontend/static \
    LOCALES_DIR=/app/locales \
    PLUGINS_DIR=/app/plugins \
    BIND_ADDR=0.0.0.0 \
    PORT=5173 \
    LOG_FORMAT=json

USER ferum
EXPOSE 5173

# Migrations run inside the process at startup, so there is no separate migrate
# step. SIGTERM triggers a graceful drain, which is what `docker stop` sends.
CMD ["ferum-board"]
