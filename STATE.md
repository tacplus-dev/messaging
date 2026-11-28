# State: messaging

Overwritten to stay current (loopers and agents). Last updated: 2026-11-28 by the looper (#7).

- **Version / line:** v0.1.1 (line 0.1); tacplus floor 2.6, pinned to 2.6.1 and its build image by digest in `tacplus.lock` (#12)
- **Works:** `SendMessage` writes its N deliveries to Stratum through the record write surface as one `Apply` (all or nothing), routed (`target_tenant`) to the sender's tenant, then announces them on `messaging.MessageBus.stream` for mailbox; nothing is published for Stratum to store (#7). Store and topic both carry the inbound `trace_id` (#11, #7); `just gates` passes on tacplus 2.6.1 (#12, #7: 27 tests; the aws-lc-rs chassis on the ed25519-dalek 3 / der 0.8 / sha2 0.11 RustCrypto line); `cargo audit` reports no vulnerabilities (#10, #9, #5)
- **Released / deployed:** source tags v0.1.0 and v0.1.1 on main; binaries, images and the GitHub release come from the build pipeline (tacplus-dev/looper#17)
- **Known gaps:** the store path is tested against a fake Stratum only, not a live per-tenant one; `Message.spiffe_id`/`spiffe_actor` are stored empty, as nothing stamps them on the write surface; a sender in no tenant (`/system`) cannot send (PROPOSALS.md)
- **Blocked on:** nothing; the v0.1.2 patch (#7) is cut once the land merges
