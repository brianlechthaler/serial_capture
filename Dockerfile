# syntax=docker/dockerfile:1

FROM rust:1.88-bookworm AS build
WORKDIR /workspace
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/workspace/target \
    cargo build --release --locked \
    && cp target/release/serial-capture /workspace/serial-capture

FROM debian:bookworm-slim
COPY --from=build /workspace/serial-capture /usr/local/bin/serial-capture
ENTRYPOINT ["serial-capture"]
