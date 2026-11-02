# History: messaging

Append-only: add entries at the end, never edit or remove one (CI enforces it). One line each:
`- <YYYY-MM-DD> <who: looper|agent|jody> <kind: issue|decision|release|deploy> <one line> (<links>)`

- 2026-10-29 looper issue Gates: `just gates` fails on main — stratum needs tacplus 2.5; move the floor from 2.4 to 2.5 (#8, b2fb7a5)
- 2026-10-30 looper issue Gates: `just gates` fails on main — Cargo.lock is stale against the tacplus 2.5.2+ chassis (aws-lc-rs); relock (#9, 432ad31)
- 2026-11-02 looper issue Gates: `just gates` fails on main — Cargo.lock is stale against the tacplus 2.5.6 chassis (ed25519-dalek 3 / der 0.8 / sha2 0.11); relock (#10, 4e90210)
