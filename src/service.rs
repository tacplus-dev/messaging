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
//! The OTHER fan-out axis — who consumes the topic — is free and appears
//! nowhere in this file. It is arranged entirely by `s2s_subscribe` on the
//! record declaration, because consumer groups broadcast independently. Adding
//! a consumer is a proto edit, never a change here.
//!
//! HOW A DELIVERY IS STORED, since 2026-11-28 (#7): through stratum's generated
//! write surface, not by publishing on the record's topic. Stratum no longer
//! ingests from the bus (tacplus-dev/stratum#2), and its answer is final:
//! done, acked at the record's durability (LOCAL), or refused with the reason.
//! The deliveries of one submission are ONE `Apply`, so the fan-out is stored
//! all or nothing. Only then are they published on the topic, which now
//! stores nothing and is the event mailbox consumes.

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
use messaging_model::types::vservices::query::{
    record_op, InsertIfAbsentMessageRequest, RecordApplyRequest, RecordOp,
};

/// The writes this service makes of the record store.
///
/// A SEAM, the same one mailbox's `MailboxEntryStore` is: the record plane is
/// reachable only through a live client, and its full server trait covers
/// every record in the build. So the production implementation below is the
/// only place the generated client is named, and the tests substitute a store
/// of their own.
#[async_trait::async_trait]
pub trait MessageStore: Send + Sync {
    /// Insert every one of `deliveries`, all or nothing, as ONE `Apply` of
    /// `InsertIfAbsentMessage` ops routed to `tenant`, the tid of the
    /// per-tenant Stratum that holds them (`target_tenant`, tacplus#207). The
    /// deliveries come in the store's canonical order (by key), which an
    /// `Apply` must be in. Answers each op's revision, or the store's refusal:
    /// ALREADY_EXISTS when a key is present, UNAVAILABLE while its redo is
    /// unreachable, FAILED_PRECONDITION for a durability it does not serve yet.
    /// `trace_id` is the causing request's (field 200), carried as it came.
    async fn insert_all(
        &self,
        tenant: &str,
        deliveries: Vec<Message>,
        trace_id: &str,
    ) -> Result<Vec<i64>, tonic::Status>;
}

/// The tid of the tenant whose Stratum holds the rows of `namespace`: the
/// `<tid>` of `/tenants/<tid>` or anything below it, or `None` for a namespace
/// in no tenant (`/system`, say). The chassis's own reading of a namespace
/// claim, so this service cannot disagree with the router about it.
pub fn tenant_of(namespace: &str) -> Option<String> {
    servicemanager::tenant_route::tenant_of_caller_namespace(namespace)
}

/// The production store: stratum's record plane, through the generated client.
///
/// Nothing here constructs a client, names an address, or acquires a token:
/// `(spec.grpc_s2s_client)` in messaging.proto is what caused the client to be
/// generated and wired, and `Message`'s `s2s_write` grant is what lets this
/// service call it. The write asks for no durability, so stratum's generated
/// wrapper holds it to the record's declared floor (LOCAL).
pub struct RecordPlaneStore(pub Arc<messaging_model::s2sm::ServiceManager>);

#[async_trait::async_trait]
impl MessageStore for RecordPlaneStore {
    async fn insert_all(
        &self,
        tenant: &str,
        deliveries: Vec<Message>,
        trace_id: &str,
    ) -> Result<Vec<i64>, tonic::Status> {
        let ops = deliveries
            .into_iter()
            .map(|m| RecordOp {
                op: Some(record_op::Op::InsertIfAbsentMessage(
                    InsertIfAbsentMessageRequest {
                        item: Some(m),
                        target_tenant: tenant.to_string(),
                        trace_id: trace_id.to_string(),
                        ..Default::default()
                    },
                )),
                ..Default::default()
            })
            .collect();
        Ok(self
            .0
            .s2s()
            .stratum()
            .record_map_client()
            .apply(RecordApplyRequest {
                ops,
                target_tenant: tenant.to_string(),
                trace_id: trace_id.to_string(),
                ..Default::default()
            })
            .await?
            .into_inner()
            .results
            .into_iter()
            .map(|r| r.revision)
            .collect())
    }
}

/// The publisher the generated ServiceManager hands over for the topic
/// synthesized from `Message`'s `spec.record` (`messaging.MessageBus.stream`).
///
/// Fire-and-forget into a bounded queue: the closure reports queue backpressure
/// (`TrySendError::Full`) to the caller rather than dropping. The publish stores
/// nothing (the store write before it does); it is the event mailbox consumes.
/// See `send_message` for what a full queue answers.
pub type MessagePublisher = Arc<
    dyn Fn(Message) -> Result<(), tokio::sync::mpsc::error::TrySendError<Message>> + Send + Sync,
>;

pub struct MessagingService {
    /// Where a delivery is STORED: stratum's write surface. A send is done when
    /// this answers, and refused when it refuses.
    store: Arc<dyn MessageStore>,

    /// Where a stored delivery is ANNOUNCED. Never a hand-rolled EventBus
    /// client: this closure is generated from the record declaration and
    /// already owns the transport, the topic name, the s2s token acquisition
    /// and the connection to the state store. There is nothing left for
    /// hand-written code to get wrong, and writing a second one would mean
    /// writing all four of those again.
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
    pub fn new(store: Arc<dyn MessageStore>, publish: MessagePublisher) -> Self {
        Self {
            store,
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
///     same recipients regenerate the same delivery ids, so the store refuses
///     the retry's inserts (ALREADY_EXISTS) instead of creating a second set.
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
            // Message.recipient is (spec.assert_max_size) = 256 in the proto.
            // Checked here, not left to eventbus: a publish eventbus rejects
            // for this is retried forever by the generated publish loop, which
            // wedges that queue slot permanently rather than dropping it —
            // every later send from every tenant then queues behind it.
            if r.len() > 256 {
                return Err(tonic::Status::invalid_argument(format!(
                    "recipient {r:?} exceeds the 256-byte limit"
                )));
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

        // The per-tenant Stratum that holds this send: the sender's tenant, the
        // one its rows are stamped with below. Routed by the RECORD's tenant
        // (tacplus#207), never this service's own: messaging runs in /system
        // and writes for every tenant. A sender in no tenant has no Stratum to
        // hold the message, so the send is refused before anything is written.
        let Some(tenant) = tenant_of(&namespace) else {
            return Err(tonic::Status::failed_precondition(
                "the sender's namespace is in no tenant; there is no store to hold the message",
            ));
        };

        // One clock and one id for the whole submission: the N deliveries were
        // ONE act, so they must not disagree about when it happened or about
        // which submission they belong to. Taken once, here, rather than per
        // recipient.
        let message_id = uuid::Uuid::new_v4().to_string();
        let sent_at_ms = now_ms();
        let request_id = request_id(caller).to_string();

        // THE FAN-OUT. One delivery per recipient, each a complete Message
        // differing only in `recipient` and the `delivery_id` derived from it.
        //
        // Not one record with a repeated recipient list — that is the shape
        // EventBus's model would have made cheap and the decision doc rules out:
        // a single record is consumed once per consumer group, so a consumer would
        // get one record for three people and would have to fan it out itself,
        // with no per-recipient offset to ACK against and nothing to redeliver
        // if it crashed after handling two of the three.
        //
        // `recipients` (the To: line) IS carried on every delivery, identically.
        // That is not the fan-out — it is what lets each recipient see who else
        // was addressed.
        let deliveries: Vec<Message> = recipients
            .iter()
            .map(|recipient| Message {
                delivery_id: delivery_id(&message_id, recipient),
                message_id: message_id.clone(),
                sender: sender.clone(),
                recipient: recipient.clone(),
                recipients: recipients.clone(),
                subject: req.subject.clone(),
                text: req.text.clone(),
                ticket_id: req.ticket_id.clone(),
                sent_at_ms,
                // spiffe_id and spiffe_actor are `(spec.identity_spiffe_subject)`
                // and `(spec.identity_spiffe_actor)`: stamped from a verified
                // token by the platform, never by this service, and
                // deliberately left empty here. Writing a value into either
                // would be this service asserting an identity that nothing
                // verified, which is exactly what those annotations exist to
                // prevent. (eventbus stamped them on publish; nothing stamps
                // them on the write surface yet: PROPOSALS.md.)
                spiffe_id: String::new(),
                spiffe_actor: String::new(),
                request_id: request_id.clone(),
                // Each delivery is caused by this request, so it carries the
                // inbound trace_id (looper#51). The wrapper only reads it for
                // its own trace events; propagating it is ours. Empty stays empty.
                trace_id: req.trace_id.clone(),
                // Both receipts are written by CONSUMERS after delivery, never by
                // the sender and never here. Unset is the correct state of a
                // message that has just been sent.
                agent_received: None,
                human_viewed: None,
                // namespace is set to the WRAPPER-VERIFIED sender's namespace,
                // not left empty — FIXED 2026-09-16. Leaving it empty filed every
                // message under messaging's own service namespace instead of the
                // sender's tenant, so the intended recipient's tenant-scoped
                // read could never find it — messages sent, never delivered, no
                // error. Since #7 it is also what the write is routed by
                // (`tenant` above), and the tenant's store refuses a row
                // outside it.
                namespace: namespace.clone(),
                ..Default::default()
            })
            .collect();
        let delivery_ids: Vec<String> = deliveries.iter().map(|m| m.delivery_id.clone()).collect();

        // STORE, all or nothing: ONE Apply of the N inserts, so there is no
        // partial fan-out — a refused Apply wrote none of them. Its ops must be
        // in the store's canonical order (record type, then key), so they are
        // sorted by key; the response keeps the recipients' order.
        let mut ordered = deliveries.clone();
        ordered.sort_by(|a, b| a.delivery_id.cmp(&b.delivery_id));
        if let Err(status) = self.store.insert_all(&tenant, ordered, &req.trace_id).await {
            tracing::error!(
                code = ?status.code(),
                message_id = %message_id,
                deliveries = delivery_ids.len(),
                "message store write refused; nothing stored"
            );
            return Err(match status.code() {
                // The message_id is a fresh uuid, so a present key is a
                // collision, not a resend: the caller's answer is to retry.
                tonic::Code::AlreadyExists => tonic::Status::aborted(format!(
                    "message {message_id} collided with a stored delivery; nothing was stored, retry the send"
                )),
                // UNAVAILABLE (the store's redo is unreachable),
                // FAILED_PRECONDITION (a durability it does not serve yet) and
                // the rest are the caller's answer as they are.
                _ => status,
            });
        }

        // ANNOUNCE, for mailbox: one publish per delivery, only once the store
        // has acked them, so no consumer hears of a message that is not
        // stored.
        for (announced, message) in deliveries.into_iter().enumerate() {
            if let Err(e) = (self.publish)(message) {
                // The message IS sent: every delivery is stored and readable.
                // What failed is telling mailbox, so it is reported rather than
                // swallowed — a recipient whose mailbox never lists it would
                // not know to look. A retry would store a second copy, so the
                // answer says the first is there.
                tracing::error!(
                    error = %e,
                    message_id = %message_id,
                    announced,
                    of = delivery_ids.len(),
                    "message publish queue full; deliveries stored, announcement incomplete"
                );
                return Err(tonic::Status::unavailable(format!(
                    "message {} is stored for all {} recipients, but the publish queue was full \
                     after announcing {} of them; do not resend it",
                    message_id,
                    delivery_ids.len(),
                    announced
                )));
            }
            self.published.fetch_add(1, Ordering::Relaxed);
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
