# Runtime base per #2 (hub, 2026-10-27): distroless, pinned by digest, never a
# floating tag (#24; tests/dockerfile.rs), so a rebuild of the same commit ships
# the same, scanned base. Nightly maintenance moves the digest (tacplus-dev/looper#234).
FROM gcr.io/distroless/cc-debian13:latest@sha256:dac90a25ffe03ebaace90cda886b081e61ab6cd77ac47eda51f7bcc15c320765
COPY target/release/messaging /usr/local/bin/messaging
ENTRYPOINT ["messaging"]
