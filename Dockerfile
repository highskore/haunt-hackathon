# syntax=docker/dockerfile:1
# Builds haunt-replay on Linux, then runs its tests and every trace with no network.
#
# With haunt-core as a path dependency (Cargo.toml's default), pass the haunt checkout:
#   docker build --build-context haunt=../haunt-linux -t haunt-replay .
# With the git dependency in Cargo.toml instead, no local copy is needed:
#   docker build -t haunt-replay .
# Then:
#   docker run --rm --network none haunt-replay

# Empty placeholders unless --build-context haunt=<path> replaces this stage.
FROM busybox:1.36.1 AS haunt
RUN mkdir -p /core /wit /design && touch /Cargo.toml /design/tokens.json

FROM rust:1.98.0-slim-bookworm
# No debug info or incremental cache: a smaller image and a faster build.
ENV CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0
# Only what haunt-core builds from: its crate, the workspace manifest it inherits from, the plugin
# WIT and the design tokens it embeds.
COPY --from=haunt /Cargo.toml /src/haunt-linux/Cargo.toml
COPY --from=haunt /core /src/haunt-linux/core
COPY --from=haunt /wit /src/haunt-linux/wit
COPY --from=haunt /design/tokens.json /src/haunt-linux/design/tokens.json
WORKDIR /src/haunt-hackathon
COPY . .
RUN cargo fetch --locked
RUN cargo test --locked --offline --no-run && cargo build --locked --offline
ENV CARGO_NET_OFFLINE=true
CMD ["sh", "-c", "cargo test --locked --offline && for t in traces/*.jsonl; do echo; echo \"== haunt-replay $t\"; target/debug/haunt-replay \"$t\"; done"]
