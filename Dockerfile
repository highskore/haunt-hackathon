# syntax=docker/dockerfile:1
# Builds haunt-replay on Linux, then runs its tests and every trace with no network.
#
# With haunt-core and the MULTIMAXXING engine as path dependencies (Cargo.toml's default), pass both checkouts:
#   docker build --build-context haunt=../haunt --build-context multimaxxing=../haunt-plugins/haunt-multimaxxing -t haunt-replay .
# With the git dependency in Cargo.toml instead, no local copy is needed:
#   docker build -t haunt-replay .
# Then:
#   docker run --rm --network none haunt-replay

# Empty placeholders unless --build-context haunt=<path> / multimaxxing=<path> replace these stages.
FROM busybox:1.36.1 AS haunt
RUN mkdir -p /core /wit /design && touch /Cargo.toml /design/tokens.json
FROM busybox:1.36.1 AS multimaxxing
RUN mkdir -p /crates/engine && touch /Cargo.toml

FROM rust:1.98.0-slim-bookworm
# No debug info or incremental cache: a smaller image and a faster build.
ENV CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0
# Only what haunt-core builds from: its crate, the workspace manifest it inherits from, the plugin
# WIT and the design tokens it embeds.
COPY --from=haunt /Cargo.toml /src/haunt/Cargo.toml
COPY --from=haunt /core /src/haunt/core
COPY --from=haunt /wit /src/haunt/wit
COPY --from=haunt /design/tokens.json /src/haunt/design/tokens.json
# The engine crate and the workspace manifest it inherits from.
COPY --from=multimaxxing /Cargo.toml /src/haunt-plugins/haunt-multimaxxing/Cargo.toml
COPY --from=multimaxxing /crates/engine /src/haunt-plugins/haunt-multimaxxing/crates/engine
WORKDIR /src/haunt-hackathon
COPY . .
RUN cargo fetch --locked
RUN cargo test --locked --offline --no-run && cargo build --locked --offline
ENV CARGO_NET_OFFLINE=true
CMD ["sh", "-c", "cargo test --locked --offline && for t in traces/*.jsonl; do echo; echo \"== haunt-replay $t\"; target/debug/haunt-replay \"$t\"; done"]
