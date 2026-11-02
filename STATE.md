# State: messaging

Overwritten to stay current (loopers and agents). Last updated: 2026-11-02 by the looper (#10).

- **Version / line:** unreleased (no vX.Y.Z tag); tacplus floor 2.5
- **Works:** `SendMessage` fan-out onto `messaging.MessageBus.stream`; `just gates` passes on the latest 2.5.x compiler (2.5.6, aws-lc-rs chassis on the ed25519-dalek 3 / der 0.8 / sha2 0.11 RustCrypto line; 18 tests); `cargo audit` reports no vulnerabilities (#10, #9, #5)
- **Released / deployed:** not released
- **Known gaps:** records still reach Stratum through the event bus, which the new Stratum no longer ingests (#7)
- **Blocked on:** #7 waits on tacplus-dev/tacplus#207
