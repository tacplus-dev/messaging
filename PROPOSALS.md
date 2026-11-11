# Proposals: messaging

Written by loopers. Each proposal: what, why, links. Jody promotes accepted ones into ROADMAP.md (via an agent);
the looper then marks it `promoted` or `declined` here with the date.

## Open
- **<title>** (<date>, <issue/run link>): <what and why>
- **ROADMAP.md: the floor is 2.5, not 2.4** (2026-10-29, #8): ROADMAP.md's "Now" still says "tacplus floor 2.4"; #8 moved `tacplus.yaml` to 2.5 because stratum, a `depends`, now needs 2.5; #12 (2026-11-07) moved it on to 2.6, the latest released line, pinned in `tacplus.lock`. ROADMAP.md changes only through an agent at Jody's request, so this asks for that edit.
- **Unmaintained crates in the generated chassis** (2026-10-30, #9): after the relock `cargo audit` reports no vulnerabilities but 4 unmaintained warnings, all from generated chassis crates: `backoff` 0.4.0 and `instant` 0.1.13 (RUSTSEC-2025-0012, RUSTSEC-2024-0384; via `chassis/httpclient`), `rustls-pemfile` 2.2.0 (RUSTSEC-2025-0134; `httpclient`, `servicemanager`, `messaging-model`), `unsafe-libyaml` 0.2.11 via `serde_yaml` 0.9 deprecated (RUSTSEC-2024-0449; `grpcconfig`, `service_im`). Generated code is fixed in tacplus, so this asks for a tacplus issue to replace them (e.g. `backon`, `rustls-pki-types` PEM parsing, a maintained YAML crate), then a relock here.
- **looper-trace does not see publisher-closure handlers** (2026-11-03, #11): `looper-trace check` reports `pass (0 handlers, 0 findings)` here because `send_message`'s downstream calls go through the generated EventBus publisher closure (`create_messaging_message_bus_stream_publisher`, called as `(self.publish)(message)`), not a `.s2s()`/`.grpc()` client; the dropped `trace_id` it missed was fixed in #11. This asks for a tacplus-dev/looper issue to teach it generated publishers as causal calls.
- **tacplus still declares rmcp 3.4.1 as the generated floor** (2026-11-11, #15): the generated `messaging-model` requires `rmcp = "3.4.1"` (tacplus v2.6.1 and main, `rmcpCrateVersion` in `pkg/internal/rust/types/mcp_dispatch_gen.go`), which admits the versions RUSTSEC-2026-0457, -0461 and -0462 affect; #15 relocked this repository to 3.9.0, but any lock regenerated from scratch, or another tacplus consumer, can still resolve a vulnerable rmcp. This asks for a tacplus issue to raise the generated requirement to `3.9.0` (and its `mcp-floor-version` gate with it).

## Promoted / declined
