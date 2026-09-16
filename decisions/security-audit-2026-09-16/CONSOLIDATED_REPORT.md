# messaging Security & Quality Audit — Consolidated Report
2026-09-16 — multimodal review: Codex CLI (GPT), Grok 4.6 (via GitHub Copilot CLI), Fable 5.1 (Claude)
Scope: `src/service.rs` + `src/main.rs`, a small Rust platform service (2 files) — the platform's
person-to-person message primitive, fan-out over eventbus.

**Clean on security** — all three tools agree authorization and identity binding are handled
entirely by the generated wrapper before the handler runs; no bypass, no injection found.

## High — every message landed in messaging's own tenant, not the sender's [VERIFIED, FIXED]

Found independently by Codex and Fable (Fable with exact precision — cited the specific eventbus
commit and decision doc). **I read the code, confirmed it, and fixed it** — this was severe enough
and simple enough to fix immediately rather than only report.

**The bug:** `src/service.rs:245` (before the fix) built every fan-out `Message` with
`namespace: String::new()`, deliberately left empty. The comment there described
`(spec.store_namespace)` as pure "stamp-or-refuse at the broker" — true of eventbus's behavior
*before* `decisions/namespace-fix-corrected-2026-09-02.md` landed (same fix we personally verified
in the eventbus audit two days ago): the semantics are now a three-way — already set on the row →
accepted as the sender's own namespace; empty → stamped from the **publisher's own token**, not the
sender's. Since messaging service's own token carries messaging's own (`/system`) namespace, every
message published with an empty namespace field landed there instead of the sender's tenant.

**Consequence:** not a cross-tenant leak — nothing is exposed to the wrong tenant — but every
message becomes **silently undeliverable**. The intended recipient's tenant-scoped read (`exact
equality` on their own namespace claim) never finds a row stamped `/system`. `SendMessage` returns
success; the recipient never sees anything; no error anywhere in the path. Fable additionally traced
this into `mailbox`, which correctly copies `msg.namespace` from the delivery on purpose — so a
held-mail entry built from one of these messages is *also* mistenanted and filtered out for the
recipient there too.

**Fix applied:** `namespace: namespace.clone()` — the wrapper-verified sender's own namespace,
captured at the top of the handler and previously used only in a log line. The stale comment
describing the old stamp-or-refuse behavior is corrected in place. The one test that asserted the
empty value (`service_tests.rs`, cementing the bug) is fixed to assert the sender's namespace
instead. **All 16 tests pass** after the fix; `spiffe_id`/`spiffe_actor` are correctly left for
eventbus to stamp (that part of the original comment was accurate and untouched).

## High — one oversized recipient can wedge the entire service [cross-confirmed, Codex + Fable]

**File:** `src/service.rs:162-173` validates recipient *count* but not the length of individual
recipient strings (the proto's own `Message.recipient` field has a 256-byte limit, `delivery_id`
512 bytes — neither checked here). Fable traced further: eventbus's own ingest validation *does*
reject an oversized recipient with `INVALID_ARGUMENT`, but the generated publish loop
(`s2sm/mod.rs:1536-1549`) treats every publish error as transient and **retries the same message
forever** with backoff — so a single caller sending one 300-byte recipient wedges that slot at the
head of the (256-slot) queue permanently. Every later send from every tenant queues behind it; once
the queue fills, `SendMessage` returns `UNAVAILABLE` for everyone until the service restarts. Fix
shape: validate each built `Message` (or bound recipient length) before enqueueing, refusing with
`INVALID_ARGUMENT` the same way an empty recipient already is.

## Medium — delivery success is reported before anything reaches the broker [Fable]

**File:** `src/service.rs:249-264`. The RPC returns `message_id` and N `delivery_ids` as soon as
each message is accepted into an **in-memory** queue drained by a background task — not once
anything actually reaches eventbus. A process restart with queued deliveries, or any downstream
publish refusal, means the caller holds delivery ids for messages that never existed, with no way to
find out. The design may be an acceptable tradeoff, but the code's own comment ("a failed publish is
a failed send, and it is reported") overstates what's actually guaranteed — worth either tightening
the comment or making the RPC not return success until the queue confirms.

## Medium — partial fan-out can only be retried as a full duplicate [cross-confirmed, Codex + Fable + Grok]

**File:** `src/service.rs:197, 249-277`. `message_id` is a fresh UUID every call, with no way to
resume a specific fan-out. If a queue-full error hits after *k* of *N* recipients are already
enqueued, the caller's only recourse (a retry, which the error explicitly recommends) mints an
entirely new `message_id` and re-delivers to the first *k* recipients a second time. Grok separately
notes every `TrySendError` variant (including `Closed`, meaning the publisher task is actually gone)
is reported identically as "queue full," so a caller retries into a permanently dead channel the
same way it would retry into transient backpressure.

## Low
- `max_recipients as usize` doesn't clamp a negative config value — a misconfigured negative bound
  becomes a huge one instead of failing safe (Grok).
- `check_health` always reports `Healthy`, so a fully saturated publish queue still accepts new
  traffic with no signal to orchestration (Grok).

## Recommended next step
The namespace fix is already applied and tested — nothing further needed there. Next: the oversized-
recipient wedge, since it's a full-service outage triggerable by any single entitled caller with one
malformed request, and it's a small, well-scoped validation to add.

## Raw per-tool output
- Codex: `codex.out.md` (3 findings, correctly flagged the namespace bug independently)
- Grok 4.6: `grok.out.md` (4 findings — did not independently surface the namespace bug)
- Fable 5.1: `fable.out.md` (4 findings — the most precise on the namespace bug, cited the exact
  eventbus commit and decision doc; also found the oversized-recipient wedge mechanism in full)
