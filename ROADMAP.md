# Roadmap: messaging

Owned by Jody; changed only by an agent at Jody's request. Loopers propose in PROPOSALS.md.

## Now
The platform's message primitive: one RPC, `SendMessage`, fanning a user's message out to one `Message` per recipient on `messaging.MessageBus.stream`.
Rust. Unreleased; tacplus floor 2.4.

## Next
- Nothing approved and unblocked; every open approved issue is in Later.

## Later
- Release v0.1.0 and publish its image (#2), blocked on tacplus-dev/looper#12
- rsa and crypto-bigint advisories via the generated chassis (#5), blocked on tacplus-dev/tacplus#186
- Decide whether messaging is a platform service and, if so, prove it in seatrials (a message sent, held, pulled) (tacplus-dev/roadmap:decisions/tacplus-dev-stable-gate-2026-10-25.md)

## Not doing
- Nothing recorded yet.
