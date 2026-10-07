# syntax=docker/dockerfile:1
# Headless kliknload (no menu bar, no clipboard). Config lives in /config.

FROM rust:1-bookworm AS build
WORKDIR /src

# Cache dependencies separately from the sources.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs \
    && cargo build --release --locked --no-default-features \
    && rm -rf src target/release/kliknload target/release/deps/kliknload-*

COPY src ./src
COPY assets ./assets
RUN cargo build --release --locked --no-default-features \
    && strip target/release/kliknload

FROM debian:bookworm-slim
# ca-certificates for HTTPS outputs, a shell for command outputs.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home-dir /config --shell /usr/sbin/nologin kliknload \
    && mkdir -p /config && chown kliknload /config

COPY --from=build /src/target/release/kliknload /usr/local/bin/kliknload

ENV KLIKNLOAD_CONFIG=/config/pyloadConfig.json \
    KLIKNLOAD_LISTEN=0.0.0.0:9666 \
    RUST_LOG=info
VOLUME /config
EXPOSE 9666
USER kliknload
HEALTHCHECK --interval=30s --timeout=3s CMD curl -fsS http://127.0.0.1:9666/jdcheck.js >/dev/null || exit 1
ENTRYPOINT ["/usr/local/bin/kliknload", "--headless"]
