//! Round-trip and paired-control tests for the Messaging service.
//!
//! WHAT THESE DRIVE, and why it is the generated `ValidationWrapper` rather than
//! the handler trait. The two claims this task has to prove are split across the
//! seam:
//!
//!   - The FAN-OUT (N recipients -> N separately-keyed publishes) is the
//!     handler's, in `service.rs`.
//!   - The NAMESPACE and PRINCIPAL paired controls are tacplus's, rendered into
//!     the wrapper's `send_message` from `(spec.namespace)` and
//!     `(spec.principal)` and rendered NOWHERE else.
//!
//! A test that called the handler directly could only ever prove the first, and
//! would quietly assert that the service re-implements the second — which it
//! deliberately does not. So every test here goes in through the wrapper, with a
//! signed bearer on the request metadata, exactly as a real client's call
//! arrives.
//!
//! WHAT THESE DO NOT PROVE. The publisher here is a capturing closure standing
//! in for the generated EventBus publisher. It shows the real handler's real
//! output — the actual `Message` values, the actual count, the actual keys —
//! but it cannot show that a broker accepted them. That half is proven
//! separately and for real against the live cluster's embedded Kafka; see
//! workers/big8-p1-messaging-result.md.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use messaging_model::s2sm::internal::grpc::servers::services::messaging::messaging::{
    Authorizer, ValidationWrapper,
};
use messaging_model::types::services::messaging::messaging_server::Messaging;
use messaging_model::types::services::messaging::{Message, SendMessageRequest};
use messaging_model::types::sys::accounting::accounting::Event;

use crate::service::MessagingService;

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The tenant every fixture token carries. Nothing asserts on the literal except
/// the namespace tests, which need a SECOND value to disagree with it.
const TENANT: &str = "/acme/eng";
const OTHER_TENANT: &str = "/acme/sales";

// ---------------------------------------------------------------------------
// The generated seam's two collaborators
// ---------------------------------------------------------------------------

/// The authorizer the wrapper calls before every RPC.
///
/// A real deployment's authorizer verifies the token's signature against the
/// issuer's JWK and checks entitlements; this one only PARSES it, because the
/// thing under test is the wrapper's contract, not oauth.
///
/// Note what it does NOT supply: the namespace. The wrapper reads that off the
/// PARSED TOKEN (`Caller::namespace()`), not off this map, so an authorizer
/// cannot widen a caller's tenant by returning a different one. That is a
/// property of the generated code and the fixture is deliberately shaped so the
/// tests could not accidentally paper over it.
struct TokenAuthorizer;

impl TokenAuthorizer {
    fn identity(authorization: Option<&str>) -> Result<HashMap<String, String>, tonic::Status> {
        let parsed = authorization
            .and_then(|v| v.strip_prefix("Bearer "))
            .and_then(|t| token::token::Token::parse(t).ok())
            .ok_or_else(|| tonic::Status::unauthenticated("no parsable bearer token"))?;
        let mut identity = HashMap::new();
        identity.insert(
            "sub".to_string(),
            parsed.subject().unwrap_or_default().to_string(),
        );
        if let Some(act) = parsed.actor() {
            identity.insert("act".to_string(), act.to_string());
        }
        Ok(identity)
    }
}

#[tonic::async_trait]
impl Authorizer for TokenAuthorizer {
    async fn authorize(
        &self,
        _full_method: &str,
        authorization: Option<&str>,
    ) -> Result<HashMap<String, String>, tonic::Status> {
        Self::identity(authorization)
    }

    async fn check_at_or_below(
        &self,
        authorization: Option<&str>,
        _target_namespace: &str,
    ) -> Result<HashMap<String, String>, tonic::Status> {
        Self::identity(authorization)
    }
}

/// An accounting `Sink` that keeps the events instead of publishing them.
/// Messaging writes no accounting itself; the wrapper does, and it needs
/// somewhere to put it.
struct CapturingSink(Arc<Mutex<Vec<Event>>>);

#[tonic::async_trait]
impl am::Sink for CapturingSink {
    async fn emit(&self, event: &Event) {
        self.0.lock().unwrap().push(event.clone());
    }
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

struct Harness {
    wrapper: ValidationWrapper,
    /// Every `Message` handed to the publisher, in order. THE fan-out evidence:
    /// its length is the publish count and each element carries the record key
    /// that publish would be stored under.
    published: Arc<Mutex<Vec<Message>>>,
    svc: Arc<MessagingService>,
}

impl Harness {
    async fn start() -> Harness {
        let published: Arc<Mutex<Vec<Message>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = published.clone();
        let svc = Arc::new(MessagingService::new(Arc::new(move |m: Message| {
            sink.lock().unwrap().push(m);
            Ok(())
        })));

        // The generated seam, wired the way the ServiceManager wires it at
        // startup: an authorizer (mandatory — the wrapper fails closed without
        // one) and an accounting emitter.
        let accounting: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
        let wrapper = ValidationWrapper::new(svc.clone());
        wrapper
            .set_authorize_func(Some(Arc::new(TokenAuthorizer)))
            .await;
        wrapper.set_instance_id("testinst".to_string()).await;
        wrapper
            .set_emitter(Some(Arc::new(am::Emitter::new(
                "messaging-testinst".to_string(),
                Box::new(CapturingSink(accounting)),
            ))))
            .await;

        Harness {
            wrapper,
            published,
            svc,
        }
    }

    /// A signed bearer the wrapper's `token::Token::parse` accepts.
    ///
    /// Signed with a throwaway key because `Token::base64()` only exists once a
    /// token has been signed — nothing downstream verifies the signature, so the
    /// key is a means of SERIALISING a token, not of trusting one.
    fn bearer(subject: &str, namespace: &str, act: Option<&str>) -> String {
        let key = key::keys::generate_ed25519_key().unwrap();
        let mut t = token::token::Token::new();
        t.set_issuer("messaging-tests").unwrap();
        t.set_subject(subject).unwrap();
        t.set_expiration(now_secs() + 3600).unwrap();
        t.set_namespace(namespace).unwrap();
        if let Some(a) = act {
            t.set_actor(a).unwrap();
        }
        t.sign(&key).unwrap();
        t.base64().unwrap().to_string()
    }

    /// A `tonic::Request` carrying that bearer, exactly as a client would send
    /// one — the wrapper reads the principal and the namespace off the metadata,
    /// never off a field.
    fn signed<T>(req: T, subject: &str, namespace: &str) -> tonic::Request<T> {
        let mut request = tonic::Request::new(req);
        request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", Harness::bearer(subject, namespace, None))
                .parse()
                .unwrap(),
        );
        request
    }

    fn published(&self) -> Vec<Message> {
        self.published.lock().unwrap().clone()
    }
}

/// A request with everything but the recipients defaulted, so a test names only
/// what it is about.
fn send_to(recipients: &[&str]) -> SendMessageRequest {
    SendMessageRequest {
        recipients: recipients.iter().map(|r| r.to_string()).collect(),
        subject: "standup".to_string(),
        text: "0930 in the small room".to_string(),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// The fan-out — the claim this whole service exists to make true
// ---------------------------------------------------------------------------

/// N recipients produce N publishes, each on its OWN key.
///
/// This is the assertion the decision doc's SUPERSEDED section turns on:
/// EventBus "cannot natively turn one publish into three independently-
/// trackable, independently-ACKable per-recipient deliveries. The Messaging
/// service must do this split itself." So the test is not "a publish happened"
/// — it is a count and a set of distinct keys.
#[tokio::test]
async fn fan_out_is_one_separately_keyed_publish_per_recipient() {
    let h = Harness::start().await;

    let resp = h
        .wrapper
        .send_message(Harness::signed(
            send_to(&["user:alice", "user:bob", "user:carol"]),
            "user:dennis",
            TENANT,
        ))
        .await
        .expect("send")
        .into_inner();

    let published = h.published();

    // THREE publishes. Not one.
    assert_eq!(
        published.len(),
        3,
        "3 recipients must produce 3 publishes, not {}",
        published.len()
    );

    // THREE DISTINCT KEYS. `delivery_id` is the record's `(spec.record).key`, so
    // two records sharing one would be one row written twice — a silent
    // overwrite, since the record is CRUD-mutable for the receipts.
    let keys: std::collections::HashSet<&str> =
        published.iter().map(|m| m.delivery_id.as_str()).collect();
    assert_eq!(keys.len(), 3, "delivery ids must be distinct: {keys:?}");

    // Each key is the (message, recipient) pair it stands for, and each record's
    // `recipient` is the one it is FOR.
    for (m, expected) in published.iter().zip(["user:alice", "user:bob", "user:carol"]) {
        assert_eq!(m.recipient, expected);
        assert_eq!(m.delivery_id, format!("{}/{}", m.message_id, expected));
    }

    // ONE submission. All three deliveries agree about which act they came from
    // and when it happened — they were one send.
    let message_ids: std::collections::HashSet<&str> =
        published.iter().map(|m| m.message_id.as_str()).collect();
    assert_eq!(message_ids.len(), 1, "one submission is one message_id");
    assert_eq!(published[0].sent_at_ms, published[2].sent_at_ms);

    // The To: line rides on every delivery, identically. This is the part that
    // is NOT the fan-out: it is what lets each recipient see who else was
    // addressed.
    for m in &published {
        assert_eq!(
            m.recipients,
            vec!["user:alice", "user:bob", "user:carol"],
            "every delivery carries the full To: line"
        );
    }

    // The response reports the fan-out, so a caller can assert on it without
    // reading the topic.
    assert_eq!(resp.delivery_ids.len(), 3);
    assert_eq!(resp.message_id, published[0].message_id);
    assert_eq!(
        resp.delivery_ids,
        published
            .iter()
            .map(|m| m.delivery_id.clone())
            .collect::<Vec<_>>()
    );
}

/// The negative half, stated separately because it is the failure mode a naive
/// reading of "messaging has no opinions" produces: ONE record carrying a
/// repeated recipient list, left for the consumer to fan out.
#[tokio::test]
async fn fan_out_is_not_a_single_record_with_a_recipient_list() {
    let h = Harness::start().await;

    h.wrapper
        .send_message(Harness::signed(
            send_to(&["user:alice", "user:bob"]),
            "user:dennis",
            TENANT,
        ))
        .await
        .expect("send");

    let published = h.published();
    assert_ne!(
        published.len(),
        1,
        "a single fan-out record is exactly what this service exists to avoid"
    );
    assert_eq!(published.len(), 2);
    // And no delivery is addressed to "everyone": each names ONE recipient.
    for m in &published {
        assert!(!m.recipient.is_empty());
        assert!(m.recipients.contains(&m.recipient));
    }
}

/// One recipient is still one publish — the fan-out degenerates, it is not
/// special-cased.
#[tokio::test]
async fn single_recipient_is_one_publish() {
    let h = Harness::start().await;
    h.wrapper
        .send_message(Harness::signed(
            send_to(&["user:alice"]),
            "user:dennis",
            TENANT,
        ))
        .await
        .expect("send");
    assert_eq!(h.published().len(), 1);
}

/// A repeated recipient collapses rather than producing a colliding key.
#[tokio::test]
async fn duplicate_recipients_collapse() {
    let h = Harness::start().await;
    h.wrapper
        .send_message(Harness::signed(
            send_to(&["user:alice", "user:bob", "user:alice"]),
            "user:dennis",
            TENANT,
        ))
        .await
        .expect("send");

    let published = h.published();
    assert_eq!(published.len(), 2, "the duplicate is one delivery, not two");
    let keys: std::collections::HashSet<&str> =
        published.iter().map(|m| m.delivery_id.as_str()).collect();
    assert_eq!(keys.len(), 2);
}

/// The fan-out bound is enforced and comes from CONFIG, not from a constant.
#[tokio::test]
async fn max_recipients_bounds_the_fan_out() {
    let h = Harness::start().await;
    h.svc.set_max_recipients(2);

    let err = h
        .wrapper
        .send_message(Harness::signed(
            send_to(&["user:a", "user:b", "user:c"]),
            "user:dennis",
            TENANT,
        ))
        .await
        .expect_err("3 recipients over a bound of 2 must be refused");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);

    // Refused BEFORE any publish: a partial fan-out is not how a bound is
    // reported.
    assert!(h.published().is_empty());
}

// ---------------------------------------------------------------------------
// The namespace paired control — WRITE side
// ---------------------------------------------------------------------------

/// POSITIVE. A request with no namespace is STAMPED from the caller's verified
/// claim, and the send proceeds.
#[tokio::test]
async fn namespace_is_stamped_from_the_verified_caller() {
    let h = Harness::start().await;

    // Note the request carries no namespace at all.
    let req = send_to(&["user:alice"]);
    assert!(req.namespace.is_empty());

    h.wrapper
        .send_message(Harness::signed(req, "user:dennis", TENANT))
        .await
        .expect("a send with no namespace must be stamped, not refused");

    assert_eq!(h.published().len(), 1);
}

/// NEGATIVE, the paired half. A request naming a DIFFERENT namespace than the
/// caller's verified claim is REFUSED — the caller cannot address a send into
/// another tenant.
///
/// INVALID_ARGUMENT, not a crash: that is the WRITE-side behaviour of the
/// namespace-isolation backstop. The crash belongs to the READ side, where a row
/// of a foreign tenant reaching the egress filter means the store's own query is
/// broken and the fleet must notice. Here nothing is broken — a caller sent a
/// bad argument — so the call is refused and the process lives.
#[tokio::test]
async fn mismatched_namespace_is_refused() {
    let h = Harness::start().await;

    let mut req = send_to(&["user:alice"]);
    req.namespace = OTHER_TENANT.to_string();

    let err = h
        .wrapper
        .send_message(Harness::signed(req, "user:dennis", TENANT))
        .await
        .expect_err("a namespace other than the caller's must be refused");

    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(
        err.message().contains("namespace"),
        "refusal must name the namespace: {}",
        err.message()
    );

    // Refused BEFORE the handler ran: nothing was published into the wrong
    // tenant.
    assert!(h.published().is_empty());
}

/// A request that names the caller's OWN namespace is accepted — the control
/// refuses a mismatch, it does not refuse the field being present.
#[tokio::test]
async fn matching_namespace_is_accepted() {
    let h = Harness::start().await;
    let mut req = send_to(&["user:alice"]);
    req.namespace = TENANT.to_string();

    h.wrapper
        .send_message(Harness::signed(req, "user:dennis", TENANT))
        .await
        .expect("the caller's own namespace must be accepted");
    assert_eq!(h.published().len(), 1);
}

/// No bearer at all is UNAUTHENTICATED, not an unstamped send.
#[tokio::test]
async fn unauthenticated_send_is_refused() {
    let h = Harness::start().await;
    let err = h
        .wrapper
        .send_message(tonic::Request::new(send_to(&["user:alice"])))
        .await
        .expect_err("no bearer must be refused");
    assert_eq!(err.code(), tonic::Code::Unauthenticated);
    assert!(h.published().is_empty());
}

// ---------------------------------------------------------------------------
// The principal paired control — the same shape, bound to the subject
// ---------------------------------------------------------------------------

/// POSITIVE. `sender` left empty is stamped with the authenticated subject.
#[tokio::test]
async fn sender_is_stamped_from_the_verified_caller() {
    let h = Harness::start().await;
    h.wrapper
        .send_message(Harness::signed(
            send_to(&["user:alice"]),
            "user:dennis",
            TENANT,
        ))
        .await
        .expect("send");
    assert_eq!(h.published()[0].sender, "user:dennis");
}

/// NEGATIVE. A caller cannot send AS someone else.
#[tokio::test]
async fn spoofed_sender_is_refused() {
    let h = Harness::start().await;
    let mut req = send_to(&["user:alice"]);
    req.sender = "user:someone-else".to_string();

    let err = h
        .wrapper
        .send_message(Harness::signed(req, "user:dennis", TENANT))
        .await
        .expect_err("a sender other than the caller must be refused");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(h.published().is_empty());
}

// ---------------------------------------------------------------------------
// The shape of what gets published
// ---------------------------------------------------------------------------

/// `ticket_id` is carried as a FIELD, verbatim, and nothing parses `text`.
#[tokio::test]
async fn ticket_id_is_a_field_not_parsed_from_the_body() {
    let h = Harness::start().await;

    let mut req = send_to(&["user:alice", "user:bob"]);
    req.ticket_id = "TKT-91824".to_string();
    // A body that MENTIONS a different ticket. Nothing may read it.
    req.text = "see also ticket TKT-00000".to_string();

    h.wrapper
        .send_message(Harness::signed(req, "user:dennis", TENANT))
        .await
        .expect("send");

    for m in h.published() {
        assert_eq!(m.ticket_id, "TKT-91824");
        assert_eq!(m.text, "see also ticket TKT-00000");
    }
}

/// A message with no ticket carries an empty `ticket_id` — the reference is
/// optional and its absence is not an error.
#[tokio::test]
async fn ticket_id_is_optional() {
    let h = Harness::start().await;
    h.wrapper
        .send_message(Harness::signed(
            send_to(&["user:alice"]),
            "user:dennis",
            TENANT,
        ))
        .await
        .expect("send");
    assert!(h.published()[0].ticket_id.is_empty());
}

/// Identity fields are left for EVENTBUS to stamp, namespace is carried from
/// the wrapper-verified sender, and the two receipts are left for CONSUMERS
/// to write.
#[tokio::test]
async fn service_stamps_no_identity_carries_sender_namespace_and_no_receipts() {
    let h = Harness::start().await;
    h.wrapper
        .send_message(Harness::signed(
            send_to(&["user:alice"]),
            "user:dennis",
            TENANT,
        ))
        .await
        .expect("send");

    let m = &h.published()[0];
    // (spec.identity_spiffe_subject) / (spec.identity_spiffe_actor) are
    // stamped by eventbus's PublishHandler from the publishing token.
    assert!(m.spiffe_id.is_empty(), "eventbus stamps spiffe_id");
    assert!(m.spiffe_actor.is_empty(), "eventbus stamps spiffe_actor");
    // (spec.store_namespace) is carried from the wrapper-verified sender —
    // FIXED 2026-09-16, see the namespace field's own comment in service.rs.
    // Leaving this empty left every message stamped with messaging's OWN
    // namespace instead of the sender's tenant, so the recipient could never
    // read it.
    assert_eq!(m.namespace, TENANT, "namespace carries the sender's tenant");
    // Receipts are written after delivery, by whoever delivered or viewed.
    assert!(m.agent_received.is_none());
    assert!(m.human_viewed.is_none());
}

/// An empty recipient entry is refused rather than silently dropped: it would
/// derive a delivery_id addressed to nobody.
#[tokio::test]
async fn empty_recipient_entry_is_refused() {
    let h = Harness::start().await;
    let err = h
        .wrapper
        .send_message(Harness::signed(
            send_to(&["user:alice", "   "]),
            "user:dennis",
            TENANT,
        ))
        .await
        .expect_err("an empty recipient must be refused");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(h.published().is_empty());
}

/// A recipient over Message.recipient's (spec.assert_max_size) = 256 is
/// refused here rather than passed through to eventbus, which would retry
/// the rejected publish forever and wedge that queue slot permanently.
#[tokio::test]
async fn oversized_recipient_is_refused() {
    let h = Harness::start().await;
    let too_long = "x".repeat(257);
    let err = h
        .wrapper
        .send_message(Harness::signed(
            send_to(&[&too_long]),
            "user:dennis",
            TENANT,
        ))
        .await
        .expect_err("an oversized recipient must be refused");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(h.published().is_empty());
}

/// Re-sending the SAME submission regenerates the SAME delivery ids, which is
/// what makes the fan-out replay-safe at the store: a retry rewrites the same
/// rows rather than creating a second set.
///
/// Demonstrated at the derivation, because the API does not yet let a caller
/// supply the message_id (see the questions file) — so this asserts the property
/// the derivation gives, not an end-to-end retry.
#[tokio::test]
async fn delivery_ids_are_derived_and_therefore_stable() {
    let h = Harness::start().await;
    h.wrapper
        .send_message(Harness::signed(
            send_to(&["user:alice", "user:bob"]),
            "user:dennis",
            TENANT,
        ))
        .await
        .expect("send");

    let published = h.published();
    let mid = &published[0].message_id;
    assert_eq!(published[0].delivery_id, format!("{mid}/user:alice"));
    assert_eq!(published[1].delivery_id, format!("{mid}/user:bob"));
}
