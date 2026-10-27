# Runtime base per #2 (hub, 2026-10-27): distroless.
FROM gcr.io/distroless/cc-debian13
COPY target/release/messaging /usr/local/bin/messaging
ENTRYPOINT ["messaging"]
