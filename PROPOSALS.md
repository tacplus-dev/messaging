# Proposals: messaging

Written by loopers. Each proposal: what, why, links. Jody promotes accepted ones into ROADMAP.md (via an agent);
the looper then marks it `promoted` or `declined` here with the date.

## Open
- **Mailbox could watch the store instead of the topic** (2026-11-28, #7): messaging now writes every delivery
  through Stratum's write surface and publishes on `messaging.MessageBus.stream` only for mailbox, whose
  `(spec.event_watch)` still reads it. If mailbox moved to `WatchMessageChanges` (it is the record's `s2s_subscribe`
  audience already), messaging could drop the topic, its publisher and the "stored, but announcement incomplete"
  answer a full publish queue gives. Needs a mailbox issue and both to land before a deploy.
- **A sender in no tenant cannot send** (2026-11-28, #7): every delivery is now written to the Stratum of the
  sender's tenant (the tid of `/tenants/<tid>[/...]`), so a sender whose namespace is in no tenant (`/system`, a
  service sending mail) is refused with FAILED_PRECONDITION before anything is written. Mailbox drops such deliveries
  too (mailbox PROPOSALS.md, "Mail addressed in no tenant"). If service-to-service mail is wanted, it needs a home: a
  platform store, or the recipient's tenant.
- **Message.spiffe_id has no stamper off the bus** (2026-10-29, #7, tacplus-dev/tacplus#207): eventbus stamped
  `(spec.identity_spiffe_subject)` / `(spec.identity_spiffe_actor)` on publish. Nothing stamps them on a write-surface
  write, so the record's `owner` would be stored empty. Messaging must not fill the field in itself (messaging.proto
  says why), so the store's generated wrapper should stamp it.
- **<title>** (<date>, <issue/run link>): <what and why>
- **ROADMAP.md: the floor is 2.5, not 2.4** (2026-10-29, #8): ROADMAP.md's "Now" still says "tacplus floor 2.4"; #8 moved `tacplus.yaml` to 2.5 because stratum, a `depends`, now needs 2.5; #12 (2026-11-07) moved it on to 2.6, the latest released line, pinned in `tacplus.lock`. ROADMAP.md changes only through an agent at Jody's request, so this asks for that edit.
- **Unmaintained crates in the generated chassis** (2026-10-30, #9): after the relock `cargo audit` reports no vulnerabilities but 4 unmaintained warnings, all from generated chassis crates: `backoff` 0.4.0 and `instant` 0.1.13 (RUSTSEC-2025-0012, RUSTSEC-2024-0384; via `chassis/httpclient`), `rustls-pemfile` 2.2.0 (RUSTSEC-2025-0134; `httpclient`, `servicemanager`, `messaging-model`), `unsafe-libyaml` 0.2.11 via `serde_yaml` 0.9 deprecated (RUSTSEC-2024-0449; `grpcconfig`, `service_im`). Since then (nightly scan 2026-11-23, `20261123T094023Z-scan`) the build's cargo-deny and osv-scanner also flag `chrono` 0.4.45 as unmaintained (RUSTSEC-2026-0543, no fixed version), reached through the generated `logger`, `token`, `service_im` and through `rmcp`, `schemars`, `prost-wkt`, `pbjson-types`. Generated code is fixed in tacplus, so this asks for a tacplus issue to replace them (e.g. `backon`, `rustls-pki-types` PEM parsing, a maintained YAML crate), then a relock here.
- **looper-trace does not see publisher-closure handlers** (2026-11-03, #11): `looper-trace check` reports `pass (0 handlers, 0 findings)` here because `send_message`'s downstream calls go through the generated EventBus publisher closure (`create_messaging_message_bus_stream_publisher`, called as `(self.publish)(message)`), not a `.s2s()`/`.grpc()` client; the dropped `trace_id` it missed was fixed in #11. This asks for a tacplus-dev/looper issue to teach it generated publishers as causal calls.

## Promoted / declined
