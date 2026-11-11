# Runtime base per #2 (hub, 2026-10-27): distroless.
FROM gcr.io/distroless/cc-debian13
COPY target/release/messaging /usr/local/bin/messaging
# Unprivileged, at the fixed UID/GID a pod's securityContext names (#14);
# distroless defines 65532 (nonroot) and has no shell to add a user. The
# service writes no files, so nothing needs its ownership; the binary stays
# root-owned and world-executable.
USER 65532:65532
ENTRYPOINT ["messaging"]
