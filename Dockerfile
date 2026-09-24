# syntax=docker/dockerfile:1

FROM rust:1.98-bookworm@sha256:9a73a5088750b4c95158ab26629c854c3d6fc4b173cb7bc8079ad252d8ed7bfa AS build
WORKDIR /workspace
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/workspace/target \
    cargo build --release --locked \
    && cp target/release/serial-capture /workspace/serial-capture \
    && cp target/release/serial-capture-mcp /workspace/serial-capture-mcp

FROM debian:bookworm-slim@sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171
RUN useradd --system --uid 65532 --gid dialout --no-create-home capture
COPY --from=build /workspace/serial-capture /usr/local/bin/serial-capture
COPY --from=build /workspace/serial-capture-mcp /usr/local/bin/serial-capture-mcp
USER capture
HEALTHCHECK --interval=30s --timeout=3s CMD ["sh", "-c", "kill -0 1"]
ENTRYPOINT ["serial-capture"]
