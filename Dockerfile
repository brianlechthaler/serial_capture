# syntax=docker/dockerfile:1

FROM rust:1.88-bookworm AS build
WORKDIR /workspace
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY tests ./tests
RUN cargo build --release --locked

FROM debian:bookworm-slim
COPY --from=build /workspace/target/release/serial-capture /usr/local/bin/serial-capture
ENTRYPOINT ["serial-capture"]
