# messaging

The platform's message primitive. One RPC (`SendMessage`), and almost all of it
is recipient fan-out.

A user calls `messaging.Messaging/SendMessage` (`RPC_PRINCIPAL_USER`);
messaging publishes N separately-keyed `Message` records onto
`messaging.MessageBus.stream`, one per recipient, s2s. EventBus takes no client
connections, so this service is the only way a user gets a message onto the bus.

- Proto: `proto/services/messaging.proto`
- Language: Rust (see `workers/big8-p1-messaging-questions.md` §2 for why —
  the `(spec.namespace)` write-side control is generated for Rust only today)
- Build: `just generate` runs `tacplus build` on the floor and `depends` that
  `tacplus.yaml` declares; generated code lands only in `target/`. `just gates`
  is the check to run before pushing (and what the looper runs): it fails if
  anything under `target/` is tracked, regenerates and fails if that changes
  anything git sees, then runs `cargo build --locked --all-targets` and
  `cargo test --locked`. It exits non-zero if any step fails

See `platform/services/vanilla/FAMILY.md` for the ecosystem conventions.
