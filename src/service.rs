//! The Messaging service handler.
//!
//! ONE RPC, and almost all of it is fan-out.
//!
//! `Message` is a primitive with no opinions
//! (decisions/messaging-vs-ticketing-2026-08-31.md), so there is deliberately no
//! business logic here: no routing rules, no ticket awareness beyond copying an
//! opaque `ticket_id` through, no notion of what a message MEANS. What there IS,
//! and the only reason this service exists rather than the user publishing
//! straight to EventBus, is two things:
//!
//!   1. ACCESS. EventBus is s2s-only — the same posture stratum holds — so a
//!      user reaches the bus through a service or not at all
//!      (decisions/eventbus-messaging-backend-2026-08-31.md, SUPERSEDED
//!      section).
//!   2. RECIPIENT FAN-OUT. EventBus has no multi-key publish: one publish is one
//!      record, consumed once per consumer group. It cannot turn a message
//!      addressed to three people into three independently-consumable,
//!      independently-ACKable deliveries. This service does that split, and it
//!      is the real work on this path.
//!
//! The OTHER fan-out axis — scotty's consumer group vs. the future mailbox
//! service's — is free and appears nowhere in this file. It is arranged entirely
//! by `s2s_subscribe` on the record declaration, because consumer groups
//! broadcast independently. Adding a consumer is a proto edit, never a change
//! here.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

use messaging_model::s2sm::internal::grpc::servers::services::messaging::messaging::MessagingHandler;
use messaging_model::types::services::messaging::Config as MessagingConfig;
use messaging_model::types::services::messaging::{
    Message, SendMessageRequest, SendMessageResponse,
};
use messaging_model::types::sys::healthreport::healthreport::InstanceStatus;

/// The publisher the generated ServiceManager hands over for the topic
/// synthesized from `Message`'s `spec.record` (`messaging.MessageBus.stream`).
///
/// Fire-and-forget into a bounded queue: the closure reports queue backpressure
/// (`TrySendError::Full`) to the caller rather than dropping, which is the only
/// reason this path can report a partial fan-out at all. See `send_message` for
/// what it does with that.
pub type MessagePublisher = Arc<
    dyn Fn(Message) -> Result<(), tokio::sync::mpsc::error::TrySendError<Message>> + Send + Sync,
>;

pub struct MessagingService {
    /// The ONLY outbound path. Never a hand-rolled EventBus client: this closure
    /// is generated from the record declaration and already owns the transport,
    /// the topic name, the s2s token acquisition and the connection to the state
    /// store. There is nothing left for hand-written code to get wrong, and
    /// writing a second one would mean writing all four of those again.
    publish: MessagePublisher,

    /// max_recipients as delivered by kernel config — the fan-out bound. Held
    /// here rather than as a constant so changing it in kernel changes this
    /// service's behaviour with no redeploy.
    max_recipients: RwLock<i32>,

    /// Count of deliveries published by this instance, for the shutdown line and
    /// for health.
    published: AtomicI64,
}

impl MessagingService {
    pub fn new(publish: MessagePublisher) -> Self {
        Self {
            publish,
            // Overwritten by the first `messaging_config` call before the server
            // accepts traffic; the generated SM delivers config first. The
            // default matches spec.default_int in the proto.
            max_recipients: RwLock::new(64),
            published: AtomicI64::new(0),
        }
    }

    pub fn set_max_recipients(&self, v: i32) {
        if let Ok(mut w) = self.max_recipients.write() {
            *w = v;
        }
    }

    fn max_recipients(&self) -> i32 {
        self.max_recipients
            .read()
            .map(|v| *v)
            // A poisoned lock must not take the service down over a bound.
            .unwrap_or(64)
    }

    pub fn published(&self) -> i64 {
        self.published.load(Ordering::Relaxed)
    }
}

/// The delivery key for one (message, recipient) pair.
///
/// DERIVED, not random, and that is the whole design of the fan-out:
///
///   - It makes the N publishes of one submission N SEPARATELY-KEYED records.
///     A single fan-out record, or N records sharing one key, would both defeat
///     the point — the record store is CRUD-mutable (the two receipts are
///     written after the fact), so N rows on one key is N silent overwrites.
///   - It is reconstructible by anyone who knows (which message, for whom), so
///     an agent ACKing a delivery never has to have been told an opaque id.
///   - It makes a retried submission REPLAY-SAFE: the same message_id and the
///     same recipients regenerate the same delivery ids, so the retry rewrites
///     the same rows instead of creating a second set.
///
/// The separator is '/' rather than ':' because a recipient is a principal
/// subject and those can contain ':' (a SPIFFE id does); '/' appears in them too,
/// but the split is never performed — the id is only ever compared whole, and
/// the message_id half is a uuid of fixed shape, so an unambiguous parse is
/// available to anyone who needs one.
fn delivery_id(message_id: &str, recipient: &str) -> String {
    format!("{message_id}/{recipient}")
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// The x-request-id the generated wrapper bound to this call, or "" when the
/// call arrived with no authenticated caller. The header is extracted and
/// forwarded by the chassis, never by this service.
fn request_id(caller: Option<&servicemanager::auth::Caller>) -> &str {
    caller.map(|c| c.request_id()).unwrap_or("")
}

#[tonic::async_trait]
impl MessagingHandler for MessagingService {
    async fn send_message(
        &self,
        caller: Option<&servicemanager::auth::Caller>,
        req: SendMessageRequest,
    ) -> Result<SendMessageResponse, tonic::Status> {
        // sender is (spec.principal) and namespace is (spec.namespace): the
        // generated wrapper has already bound BOTH to the authenticated caller —
        // each stamped when the client left it empty, accepted when it matched,
        // refused with INVALID_ARGUMENT when it differed, and the whole call
        // refused as UNAUTHENTICATED when there was no caller or no namespace
        // claim. Nothing to check here; checking again would be a second,
        // divergent copy of a decision the wrapper has already made.
        let sender = req.sender;
        let namespace = req.namespace;

        // Deduplicate BEFORE bounding and before publishing. Two identical
        // entries in `recipients` would derive the same delivery_id, so they are
        // not two deliveries at all — they are one record published twice, the
        // second overwriting the first. Refusing would be the other defensible
        // answer; collapsing is chosen because addressing someone twice is a
        // typo, not an error, and the To: line a recipient sees should show each
        // party once. Order is preserved: `delivery_ids` in the response is
        // positional against the recipients as given, minus the duplicates.
        let mut recipients: Vec<String> = Vec::with_capacity(req.recipients.len());
        for r in req.recipients {
            let r = r.trim().to_string();
            if r.is_empty() {
                return Err(tonic::Status::invalid_argument(
                    "recipients must not contain an empty entry",
                ));
            }
            if !recipients.contains(&r) {
                recipients.push(r);
            }
        }

        if recipients.is_empty() {
            return Err(tonic::Status::invalid_argument(
                "recipients must name at least one recipient",
            ));
        }

        // The fan-out bound. N recipients is N publishes, so an unbounded list is
        // an unbounded amount of work bought with one RPC — this is a resource
        // limit on the service, not a rule about messages.
        let max = self.max_recipients() as usize;
        if recipients.len() > max {
            return Err(tonic::Status::invalid_argument(format!(
                "recipients: {} exceeds max_recipients ({})",
                recipients.len(),
                max
            )));
        }

        // One clock and one id for the whole submission: the N deliveries were
        // ONE act, so they must not disagree about when it happened or about
        // which submission they belong to. Taken once, here, rather than per
        // recipient.
        let message_id = uuid::Uuid::new_v4().to_string();
        let sent_at_ms = now_ms();
        let request_id = request_id(caller).to_string();

        // THE FAN-OUT. One publish per recipient, each a complete Message
        // differing only in `recipient` and the `delivery_id` derived from it.
        //
        // Not one publish with a repeated recipient list — that is the shape
        // EventBus's model would have made cheap and the decision doc rules out:
        // a single record is consumed once per consumer group, so scotty would
        // get one record for three people and would have to fan it out itself,
        // with no per-recipient offset to ACK against and nothing to redeliver
        // if it crashed after handling two of the three.
        //
        // `recipients` (the To: line) IS carried on every delivery, identically.
        // That is not the fan-out — it is what lets each recipient see who else
        // was addressed.
        let mut delivery_ids = Vec::with_capacity(recipients.len());
        for recipient in &recipients {
            let delivery_id = delivery_id(&message_id, recipient);

            let message = Message {
                delivery_id: delivery_id.clone(),
                message_id: message_id.clone(),
                sender: sender.clone(),
                recipient: recipient.clone(),
                recipients: recipients.clone(),
                subject: req.subject.clone(),
                text: req.text.clone(),
                ticket_id: req.ticket_id.clone(),
                sent_at_ms,
                // spiffe_id and spiffe_actor are STAMPED BY EVENTBUS from the
                // publishing token — `(spec.identity_spiffe_subject)` and
                // `(spec.identity_spiffe_actor)` — and are deliberately left
                // empty here. Writing a value into either would be this
                // service asserting an identity that nothing verified, which
                // is exactly what those annotations exist to prevent.
                spiffe_id: String::new(),
                spiffe_actor: String::new(),
                request_id: request_id.clone(),
                // Both receipts are written by CONSUMERS after delivery, never by
                // the sender and never here. Unset is the correct state of a
                // message that has just been sent.
                agent_received: None,
                human_viewed: None,
                // namespace is set to the WRAPPER-VERIFIED sender's namespace,
                // not left empty — FIXED 2026-09-16. The comment this replaced
                // described `(spec.store_namespace)` as pure stamp-or-refuse at
                // the broker, which was eventbus's behavior before
                // decisions/namespace-fix-corrected-2026-09-02.md: it is now a
                // three-way (already set on the row → accepted; empty →
                // stamped from the PUBLISHER's own token, not the sender's).
                // Leaving this empty meant every message landed under
                // messaging's own service namespace instead of the sender's
                // tenant, so the intended recipient's tenant-scoped read could
                // never find it — messages sent, never delivered, no error.
                // Same fix mailbox already has (mailbox/src/service.rs:396
                // copies msg.namespace from the delivery on purpose).
                namespace: namespace.clone(),
                ..Default::default()
            };

            if let Err(e) = (self.publish)(message) {
                // A FAILED PUBLISH IS A FAILED SEND, and it is reported rather
                // than swallowed. This is where messaging differs from scotty's
                // artifact publish, which is fire-and-forget because the state
                // write it accompanies already succeeded: here the publish IS the
                // send, so a dropped one is a message that silently never
                // existed.
                //
                // The partial fan-out is named explicitly. Deliveries before this
                // one are already on the topic and cannot be recalled, so
                // reporting a bare failure would be a lie in the other direction.
                // A retry is safe and is the right response: delivery_id is
                // derived, so re-sending the same submission rewrites the same
                // rows — but only if the caller reuses the message_id, which this
                // API does not yet let them do. That gap is written up in
                // workers/big8-p1-messaging-questions.md.
                tracing::error!(
                    error = %e,
                    message_id = %message_id,
                    delivered = delivery_ids.len(),
                    of = recipients.len(),
                    "message publish queue full; fan-out incomplete"
                );
                return Err(tonic::Status::unavailable(format!(
                    "publish queue full after {} of {} deliveries for message {}; \
                     the deliveries already published cannot be recalled",
                    delivery_ids.len(),
                    recipients.len(),
                    message_id
                )));
            }
            self.published.fetch_add(1, Ordering::Relaxed);
            delivery_ids.push(delivery_id);
        }

        tracing::info!(
            message_id = %message_id,
            deliveries = delivery_ids.len(),
            has_ticket = !req.ticket_id.is_empty(),
            namespace = %namespace,
            "message fanned out"
        );

        Ok(SendMessageResponse {
            message_id,
            delivery_ids,
            ..Default::default()
        })
    }
}

/// The generated `Handler` the ServiceManager registers, wrapping the service.
pub struct MessagingServiceHandler {
    svc: Arc<MessagingService>,
}

impl MessagingServiceHandler {
    pub fn new(svc: Arc<MessagingService>) -> Self {
        Self { svc }
    }
}

#[async_trait::async_trait]
impl messaging_model::s2sm::handler::Handler for MessagingServiceHandler {
    fn messaging_grpc_impl(&self) -> Arc<dyn MessagingHandler> {
        self.svc.clone()
    }

    async fn messaging_config(
        &self,
        cfg: &MessagingConfig,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Delivered before the server accepts traffic, and again on every
        // change. Holding the value rather than reading a constant is the whole
        // point of the config field.
        self.svc.set_max_recipients(cfg.max_recipients);
        tracing::info!(
            max_recipients = cfg.max_recipients,
            "messaging config applied"
        );
        Ok(())
    }

    fn check_health(&self) -> InstanceStatus {
        InstanceStatus::Healthy
    }

    async fn shutdown(&self) {
        tracing::info!(
            deliveries_published = self.svc.published(),
            "messaging shutting down"
        );
    }
}
