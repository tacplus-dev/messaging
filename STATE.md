# State: messaging

Overwritten to stay current (loopers and agents). Last updated: 2026-11-11 by the looper (#16).

- **Version / line:** v0.1.2 (line 0.1; rustix relocked to 1.1.6 for RUSTSEC-2026-0486, #16); tacplus floor 2.6, pinned to 2.6.1 and its build image by digest in `tacplus.lock` (#12)
- **Works:** `SendMessage` fan-out onto `messaging.MessageBus.stream`, each delivery carrying the inbound `trace_id` (#11); `just gates` passes on tacplus 2.6.1 (#12; the aws-lc-rs chassis on the ed25519-dalek 3 / der 0.8 / sha2 0.11 RustCrypto line; 20 tests); `cargo audit` reports no vulnerabilities (#10, #9, #5), rustix at 1.1.6 (#16)
- **Released / deployed:** source tags v0.1.0, v0.1.1 and v0.1.2 on main; binaries, images and the GitHub release come from the build pipeline (tacplus-dev/looper#17)
- **Known gaps:** records still reach Stratum through the event bus, which the new Stratum no longer ingests (#7)
- **Blocked on:** #7 waits on tacplus-dev/tacplus#207
