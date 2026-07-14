FROM rust:trixie AS chef

RUN apt-get update && apt-get install -y --no-install-recommends \
      protobuf-compiler libprotobuf-dev pkg-config libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

RUN cargo install cargo-chef --locked

FROM chef AS planner

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY striem/Cargo.toml striem/Cargo.toml

COPY lib/api/Cargo.toml lib/api/Cargo.toml
COPY lib/common/Cargo.toml lib/common/Cargo.toml
COPY lib/config/Cargo.toml lib/config/Cargo.toml
COPY lib/detection/Cargo.toml lib/detection/Cargo.toml
COPY lib/vector/Cargo.toml lib/vector/Cargo.toml

COPY lib/vector/build.rs lib/vector/build.rs
COPY lib/detection/build.rs lib/detection/build.rs

RUN mkdir -p lib/api/src && touch lib/api/src/lib.rs
RUN mkdir -p lib/common/src && touch lib/common/src/lib.rs
RUN mkdir -p lib/config/src && touch lib/config/src/lib.rs
RUN mkdir -p lib/detection/src && touch lib/detection/src/lib.rs
RUN mkdir -p lib/vector/src && touch lib/vector/src/lib.rs

RUN mkdir -p striem/src && printf 'fn main(){}\n' > striem/src/main.rs && touch striem/src/lib.rs

RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS build

WORKDIR /app

COPY --from=planner /app/recipe.json recipe.json

# Build scripts run during dependency cook: lib/vector fetches Vector's protos
# over the network, lib/detection compiles its in-tree proto (must be present).
COPY lib/vector/build.rs lib/vector/build.rs
COPY lib/detection/build.rs lib/detection/build.rs
COPY lib/detection/proto lib/detection/proto

RUN cargo chef cook --release --recipe-path recipe.json

COPY Cargo.toml Cargo.lock ./
COPY striem striem
COPY lib lib

# Builds every binary we ship: striem_api (API service) and usdetect (detection
# service, from the striem crate), plus the striem monolith.
RUN cargo build --release -p striem_api -p striem


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
COPY --from=build /app/target/release/striem_api /usr/local/bin/striem_api
COPY --from=build /app/target/release/usdetect /usr/local/bin/usdetect

ENV RUST_LOG=info
ENV STRIEM_API_UI_PATH=/usr/share/ui

# One image, two services. Defaults to the API; the detection service overrides
# the command to `usdetect` in docker-compose.
CMD ["striem_api"]
