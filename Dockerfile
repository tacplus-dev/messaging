# Runtime base per #2 (hub, 2026-10-27): distroless.
# Pinned by digest (#20, hub 2026-11-17): the image main names is the image that
# gets built. The digest is cc-debian13's `latest` index on 2026-11-17 (the
# docker-content-digest of gcr.io/v2/distroless/cc-debian13/manifests/latest);
# move it in a commit when distroless rebuilds with fixes.
# No HEALTHCHECK: the image has no shell, and health is reported through the
# chassis to Kubernetes probes (security-exceptions.yaml, DS-0026).
FROM gcr.io/distroless/cc-debian13@sha256:809d04afa1be033c441f76bb0051878c342fb76d0ea236bc6dc94b655fa0fbd6
COPY target/release/messaging /usr/local/bin/messaging
ENTRYPOINT ["messaging"]
