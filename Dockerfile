# =============================================================================
# Stage 1: CHEF — Base image with all build tools
# =============================================================================
FROM rustlang/rust:nightly-trixie AS chef

WORKDIR /app

# System build dependencies
RUN apt-get update -y \
    && apt-get install -y --no-install-recommends \
       lld clang pkg-config libssl-dev curl \
    && rm -rf /var/lib/apt/lists/*

# WASM compilation target
RUN rustup target add wasm32-unknown-unknown

# cargo-binstall for fast binary installs (seconds instead of minutes)
RUN curl -L --proto '=https' --tlsv1.2 -sSf \
    https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh \
    | bash

# cargo-chef (dependency caching) and cargo-leptos (full-stack build)
RUN cargo binstall cargo-chef -y
RUN cargo binstall cargo-leptos -y

# dart-sass standalone binary (handles amd64 and arm64)
ARG TARGETARCH=amd64
ARG DART_SASS_VERSION=1.83.4
RUN set -eux; \
    case "${TARGETARCH}" in \
        amd64) SASS_ARCH="x64" ;; \
        arm64) SASS_ARCH="arm64" ;; \
        *) SASS_ARCH="x64" ;; \
    esac; \
    curl -fsSL "https://github.com/sass/dart-sass/releases/download/${DART_SASS_VERSION}/dart-sass-${DART_SASS_VERSION}-linux-${SASS_ARCH}.tar.gz" \
    | tar -xz -C /usr/local; \
    ln -sf /usr/local/dart-sass/sass /usr/local/bin/sass

# =============================================================================
# Stage 2: PLANNER — Generate dependency recipe
# =============================================================================
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# =============================================================================
# Stage 3: BUILDER — Cache dependencies, then build the full application
# =============================================================================
FROM chef AS builder

COPY --from=planner /app/recipe.json recipe.json

# Cache native (SSR) dependencies
RUN cargo chef cook --release --no-default-features --features ssr --recipe-path recipe.json

# Cache WASM (hydrate) dependencies
RUN cargo chef cook --release --target wasm32-unknown-unknown --no-default-features --features hydrate --recipe-path recipe.json

# Copy full source
COPY . .

# Prevent any compile-time database connection attempts
ENV SQLX_OFFLINE=true

# Build the complete Leptos application (server binary + WASM bundle + SCSS)
RUN cargo leptos build --release -vv

# =============================================================================
# Stage 4: RUNTIME — Minimal production image
# =============================================================================
FROM debian:trixie-slim AS runtime

WORKDIR /app

RUN apt-get update -y \
    && apt-get install -y --no-install-recommends openssl ca-certificates \
    && apt-get autoremove -y \
    && apt-get clean -y \
    && rm -rf /var/lib/apt/lists/*

# Server binary
COPY --from=builder /app/target/release/gurujisivananda /app/gurujisivananda

# Compiled site assets (JS, WASM, CSS, static files)
COPY --from=builder /app/target/site /app/site

# Leptos reads [package.metadata.leptos] from Cargo.toml at runtime
COPY --from=builder /app/Cargo.toml /app/Cargo.toml

# Application configuration files
COPY --from=builder /app/configuration /app/configuration

ENV RUST_LOG="info"
ENV LEPTOS_SITE_ADDR="0.0.0.0:3000"
ENV LEPTOS_SITE_ROOT="site"
ENV APP_ENVIRONMENT="production"

EXPOSE 3000

CMD ["/app/gurujisivananda"]
