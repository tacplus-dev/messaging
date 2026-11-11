# State: messaging

Overwritten to stay current (loopers and agents). Last updated: 2026-11-11 by the looper (#15).

- **Version / line:** v0.1.1 (line 0.1); tacplus floor 2.6, pinned to 2.6.1 and its build image by digest in `tacplus.lock` (#12)
- **Works:** `SendMessage` fan-out onto `messaging.MessageBus.stream`, each delivery carrying the inbound `trace_id` (#11); `just gates` passes on tacplus 2.6.1 (#12; the aws-lc-rs chassis on the ed25519-dalek 3 / der 0.8 / sha2 0.11 RustCrypto line; 20 tests); rmcp relocked 3.4.1 → 3.9.0 (RUSTSEC-2026-0457, -0461, -0462; #15), after which `osv-scanner` on `Cargo.lock` reports no vulnerabilities, only the 4 unmaintained-crate warnings in PROPOSALS.md (#15, #10, #9, #5)
- **Released / deployed:** source tags v0.1.0 and v0.1.1 on main; binaries, images and the GitHub release come from the build pipeline (tacplus-dev/looper#17)
- **Known gaps:** records still reach Stratum through the event bus, which the new Stratum no longer ingests (#7)
- **Blocked on:** #7 waits on tacplus-dev/tacplus#207
