FROM rust:slim AS chef

RUN apt-get update && apt-get install -y --no-install-recommends \
      protobuf-compiler libprotobuf-dev pkg-config libssl-dev ca-certificates \
      git build-essential \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

RUN rustup component add rustc-codegen-cranelift-preview --toolchain nightly
RUN cargo install cargo-chef --locked

FROM chef AS planner

WORKDIR /app

COPY Cargo.toml Cargo.lock ./

# The workspace globs `lib/*`, so every crate under lib/ must be listed here
# (manifest + stub source) or `cargo chef prepare` fails to resolve the graph.
COPY lib/api/Cargo.toml lib/api/Cargo.toml
COPY lib/common/Cargo.toml lib/common/Cargo.toml
COPY lib/config/Cargo.toml lib/config/Cargo.toml
COPY lib/detection/Cargo.toml lib/detection/Cargo.toml
COPY lib/telemetry/Cargo.toml lib/telemetry/Cargo.toml
COPY lib/vector/Cargo.toml lib/vector/Cargo.toml

COPY lib/vector/build.rs lib/vector/build.rs

RUN mkdir -p lib/api/src && touch lib/api/src/lib.rs
RUN mkdir -p lib/common/src && touch lib/common/src/lib.rs
RUN mkdir -p lib/config/src && touch lib/config/src/lib.rs
RUN mkdir -p lib/detection/proto/src && touch lib/detection/proto/src/lib.rs
RUN mkdir -p lib/telemetry/src && touch lib/telemetry/src/lib.rs
RUN mkdir -p lib/vector/src && touch lib/vector/src/lib.rs

# The detection service is a lib + a bin; stub both so the workspace resolves.
RUN mkdir -p lib/detection/src/bin && touch lib/detection/src/lib.rs \
    && printf 'fn main(){}\n' > lib/detection/src/bin/detection.rs

RUN cargo +nightly chef prepare --recipe-path recipe.json

FROM chef AS build

WORKDIR /app

COPY --from=planner /app/recipe.json recipe.json

# The detection service depends on the rsigma engine crates via git. Use the git CLI
# so credentials work for the (private) timescale/rsigma repo; provide them with
# a BuildKit SSH/token mount, e.g.
#   docker build --ssh default ...
# or a Git credential helper. Public mirrors need no extra setup.
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true

# Build scripts run during dependency cook: lib/vector fetches Vector's protos
# over the network, lib/detection/proto compiles its in-tree proto (must be
# present). The detection service has no build script (it reuses lib/vector's
# proto types).
COPY lib/vector/build.rs lib/vector/build.rs
COPY lib/detection/build.rs lib/detection/build.rs
COPY lib/detection/proto lib/detection/proto

RUN cargo +nightly chef cook --recipe-path recipe.json

COPY Cargo.toml Cargo.lock ./
COPY lib lib

# Builds every binary we ship: striem_api (the API service) and detection (the
# rsigma-powered detection service).
RUN cargo +nightly build -p striem_api -p striem_detection


FROM node:26-trixie-slim AS ui-build

WORKDIR /ui

COPY ui/package.json ui/package-lock.json ./

RUN --mount=type=cache,target=/root/.npm npm ci

COPY ui/ .

ENV NEXT_PUBLIC_BASE_PATH=/ui
ENV NEXT_PUBLIC_API_URL=/api/1
RUN npm run build


FROM debian:trixie-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
      libssl3 ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=ui-build /ui/out /usr/share/ui
COPY --from=build /app/target/debug/striem_api /usr/local/bin/striem_api
COPY --from=build /app/target/debug/detection /usr/local/bin/detection

ENV RUST_LOG=info
ENV STRIEM_API_UI_PATH=/usr/share/ui

# One image, two services. Defaults to the API; the detection service overrides
# the command to `detection` in docker-compose.
CMD ["striem_api"]
