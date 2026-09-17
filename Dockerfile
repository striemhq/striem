FROM rust:trixie AS chef

RUN apt-get update && apt-get install -y --no-install-recommends \
      protobuf-compiler libprotobuf-dev pkg-config libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

RUN rustup component add rustc-codegen-cranelift-preview --toolchain nightly
RUN cargo install cargo-chef --locked

FROM chef AS planner

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY striem/Cargo.toml striem/Cargo.toml

# The workspace globs `lib/*`, so every crate under lib/ must be listed here
# (manifest + stub source) or `cargo chef prepare` fails to resolve the graph.
COPY lib/api/Cargo.toml lib/api/Cargo.toml
COPY lib/common/Cargo.toml lib/common/Cargo.toml
COPY lib/config/Cargo.toml lib/config/Cargo.toml
COPY lib/detection/Cargo.toml lib/detection/Cargo.toml
COPY lib/telemetry/Cargo.toml lib/telemetry/Cargo.toml
COPY lib/vector/Cargo.toml lib/vector/Cargo.toml
COPY rsigma-detection/Cargo.toml rsigma-detection/Cargo.toml

COPY lib/vector/build.rs lib/vector/build.rs
COPY lib/detection/build.rs lib/detection/build.rs

RUN mkdir -p lib/api/src && touch lib/api/src/lib.rs
RUN mkdir -p lib/common/src && touch lib/common/src/lib.rs
RUN mkdir -p lib/config/src && touch lib/config/src/lib.rs
RUN mkdir -p lib/detection/src && touch lib/detection/src/lib.rs
RUN mkdir -p lib/telemetry/src && touch lib/telemetry/src/lib.rs
RUN mkdir -p lib/vector/src && touch lib/vector/src/lib.rs

RUN mkdir -p striem/src && printf 'fn main(){}\n' > striem/src/main.rs && touch striem/src/lib.rs

# rsigma-detection is a lib + a bin; stub both so the workspace resolves.
RUN mkdir -p rsigma-detection/src/bin && touch rsigma-detection/src/lib.rs \
    && printf 'fn main(){}\n' > rsigma-detection/src/bin/rsigma-detection.rs

RUN cargo +nightly chef prepare --recipe-path recipe.json

FROM chef AS build

WORKDIR /app

COPY --from=planner /app/recipe.json recipe.json

# rsigma-detection depends on the rsigma engine crates via git. Use the git CLI
# so credentials work for the (private) timescale/rsigma repo; provide them with
# a BuildKit SSH/token mount, e.g.
#   docker build --ssh default ...
# or a Git credential helper. Public mirrors need no extra setup.
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true

# Build scripts run during dependency cook: lib/vector fetches Vector's protos
# over the network, lib/detection compiles its in-tree proto (must be present).
# rsigma-detection has no build script (it reuses lib/vector's proto types).
COPY lib/vector/build.rs lib/vector/build.rs
COPY lib/detection/build.rs lib/detection/build.rs
COPY lib/detection/proto lib/detection/proto

RUN cargo +nightly chef cook --recipe-path recipe.json

COPY Cargo.toml Cargo.lock ./
COPY striem striem
COPY lib lib
COPY rsigma-detection rsigma-detection

# Builds every binary we ship: striem_api (API service), usdetect (the legacy
# sigmars detection service, from the striem crate), the striem monolith, and
# rsigma-detection (the rsigma-powered detection service).
RUN cargo +nightly build -p striem_api -p striem -p rsigma-detection


FROM node:25-trixie-slim AS ui-build

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
COPY --from=build /app/target/debug/usdetect /usr/local/bin/usdetect
COPY --from=build /app/target/debug/rsigma-detection /usr/local/bin/rsigma-detection

ENV RUST_LOG=info
ENV STRIEM_API_UI_PATH=/usr/share/ui

# One image, multiple services. Defaults to the API; the detection service
# overrides the command to `rsigma-detection` (or `usdetect`) in docker-compose.
CMD ["striem_api"]
