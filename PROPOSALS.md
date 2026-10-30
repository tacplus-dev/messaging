# Proposals: messaging

Written by loopers. Each proposal: what, why, links. Jody promotes accepted ones into ROADMAP.md (via an agent);
the looper then marks it `promoted` or `declined` here with the date.

## Open
- **<title>** (<date>, <issue/run link>): <what and why>
- **ROADMAP.md: the floor is 2.5, not 2.4** (2026-10-29, #8): ROADMAP.md's "Now" still says "tacplus floor 2.4"; #8 moved `tacplus.yaml` to 2.5 because stratum, a `depends`, now needs 2.5. ROADMAP.md changes only through an agent at Jody's request, so this asks for that edit.
- **Unmaintained crates in the generated chassis** (2026-10-30, #9): after the relock `cargo audit` reports no vulnerabilities but 4 unmaintained warnings, all from generated chassis crates: `backoff` 0.4.0 and `instant` 0.1.13 (RUSTSEC-2025-0012, RUSTSEC-2024-0384; via `chassis/httpclient`), `rustls-pemfile` 2.2.0 (RUSTSEC-2025-0134; `httpclient`, `servicemanager`, `messaging-model`), `unsafe-libyaml` 0.2.11 via `serde_yaml` 0.9 deprecated (RUSTSEC-2024-0449; `grpcconfig`, `service_im`). Generated code is fixed in tacplus, so this asks for a tacplus issue to replace them (e.g. `backon`, `rustls-pki-types` PEM parsing, a maintained YAML crate), then a relock here.

## Promoted / declined
