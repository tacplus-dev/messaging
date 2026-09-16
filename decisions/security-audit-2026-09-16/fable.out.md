Audit complete. Everything below was verified against the generated wrapper, the generated publisher loop, eventbus's `PublishHandler`, and the mailbox sibling. Ranked by severity.

**1. HIGH — every message row lands in messaging's own tenant, not the sender's** (`src/service.rs:245`, also `:152`, `:228-236`)

The handler discards the wrapper-verified `req.namespace` (captured at line 152, used only in a log line at 288) and publishes with `namespace: String::new()`. The comment says eventbus is "stamp-or-refuse" from the token. That was true until eventbus commit 459cf45 (same day as this service's commit), which made `(spec.store_namespace)` three-way: already set on the row → accepted; empty → stamped from the publisher's own token. The generated binding for this topic wires that reader/stamper (`TypedEventBusBindings.java:244`). Kernel derives a service token's namespace from its SPIFFE subject, so with the field empty every delivery is stamped with messaging's namespace.

Failure scenario: Dennis in `/acme/eng` sends to Alice. The row is stored with messaging's namespace. Alice's read through stratum's c2s plane is filtered by exact equality on her claim, so she sees nothing (or per the proto comment the egress backstop halts the JVM). Mailbox makes it worse: `mailbox/src/service.rs:396` copies `msg.namespace` from the delivery on purpose, so the held-mail entry is also tenanted to messaging and `list_held_mail` filters it out for Alice. Nothing is leaked across tenants, but no user can ever read a message.

This is also pattern drift: mailbox (committed the same day) follows the corrected model by carrying the caller's namespace on the rows it writes. The fix the decision doc `scotty/backend/decisions/namespace-fix-corrected-2026-09-02.md` prescribes is exactly `namespace: namespace` here. The test at `src/service_tests.rs:561` asserts the empty value and cements the bug. The worker notes (`workers/big8-p1-messaging-questions.md` §1) describe the pre-fix eventbus semantics and are stale.

**2. HIGH — one oversized recipient wedges the publisher for the whole service** (`src/service.rs:162-173`, `:249`)

Request validation bounds only the recipient *count* (generated `validate()`, `helpers.rs:1441-1448`); individual recipient strings are unbounded. The handler never calls the generated `Message::validate()` before enqueueing, but eventbus does validate on ingest (`PublishHandler.java:269-291`, `Helpers::validateMessage`) and refuses `recipient` over 256 bytes with INVALID_ARGUMENT. The generated publish loop (`s2sm/mod.rs:1536-1549`) treats every error as transient and retries the *same* pending message forever with 3–30s backoff.

Failure scenario: any entitled user sends to a 300-byte recipient. The RPC returns OK. The poison message sits at the head of the single 256-slot queue and is retried indefinitely. Every later send from every tenant queues behind it; after 256 queued deliveries all `SendMessage` calls return UNAVAILABLE until restart. The handler should validate each built `Message` (or bound recipient length) and refuse with INVALID_ARGUMENT before touching the queue.

**3. MEDIUM — success is returned before any delivery reaches the broker; the "failed publish is a failed send" claim is false** (`src/service.rs:249-264`)

The closure only reports `TrySendError`; it is an in-memory queue drained by a background task. The RPC returns `message_id` and N `delivery_ids` while all N may still be unpublished. Shutdown aborts the drain task (`shutdown_owned_tasks`), and eventbus refusals never propagate back.

Failure scenario: process restarts with queued deliveries, or eventbus refuses a publish (e.g. PERMISSION_DENIED if the service token ever lacks a namespace claim). The caller holds ids for deliveries that never existed. Lost delivery reported as success. The design may be acceptable, but the comment and the proto's "delivery_ids ... lets a caller assert N deliveries" both overstate the guarantee.

**4. LOW — partial fan-out can only be retried as a new message, producing duplicates** (`src/service.rs:272-278`)

Acknowledged in the comment, but it is a concrete delivery-guarantee violation: on `Full` after k of N publishes, the client's only recourse is a new `SendMessage`, which mints a new `message_id` and re-delivers to the first k recipients. With a 256-slot queue and 64 max recipients, four concurrent large sends during a broker backoff hit this. Also minor: `TrySendError::Closed` is reported as "queue full".

Not found: no auth or principal bypass (the wrapper binds `sender` and `namespace` to the token at `messaging.rs:302-330`), no injection, no panic path on malformed input, no ordering issue (the drain is single-task sequential), and `main.rs` matches the mailbox convention exactly.
