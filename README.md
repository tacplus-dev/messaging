# messaging

The platform's message primitive. One RPC (`SendMessage`), and almost all of it
is recipient fan-out.

A user calls `messaging.Messaging/SendMessage` (`RPC_PRINCIPAL_USER`);
messaging publishes N separately-keyed `Message` records onto
`messaging.MessageBus.stream`, one per recipient, s2s. EventBus takes no client
connections, so this service is the only way a user gets a message onto the bus.

- Proto: `platform/proto/services/messaging.proto`
- Language: Rust (see `workers/big8-p1-messaging-questions.md` §2 for why —
  the `(spec.namespace)` write-side control is generated for Rust only today)
- Build: `tacplus build` via the app's `build.sh`, not a bare `just generate`
  (this service needs the app proto set alongside the platform's)

See `platform/services/vanilla/FAMILY.md` for the ecosystem conventions.
