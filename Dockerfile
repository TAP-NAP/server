FROM public.ecr.aws/docker/library/rust:1.87-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock* ./
COPY src ./src
RUN cargo build --release

FROM public.ecr.aws/docker/library/debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/tap-app-attest-server /app/tap-app-attest-server
COPY certs ./certs

ENV SERVER_ADDR=0.0.0.0:8080
ENV APPLE_APP_ATTEST_ROOT_CA_PATH=/app/certs/apple_app_attestation_root_ca.pem
EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD curl -fsS http://127.0.0.1:8080/healthz >/dev/null || exit 1

CMD ["/app/tap-app-attest-server"]
