//! Integration tests for the ordered-output HTTP server.
//!
//! All tests call handlers directly through the axum [`Router`] via
//! [`tower::ServiceExt::oneshot`] — no network is required and no running
//! f1r3node is needed.
//!
//! Test groups:
//!
//! - `GET /ordered-output/latest` — 503 on empty state, 200 with correct JSON shape
//! - `GET /ordered-output/status` — 503 on empty state, lightweight shape, no block list
//! - Staleness — `is_stale` reflects whether `computed_at_ns` is old

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::to_bytes;
use axum::http::{Request, StatusCode};
use cordial_f1r3node_adapter::ordered_output::OrderedFinalizedOutput;
use cordial_f1r3node_adapter::ordered_output_server::{
    OrderedOutputServerState, ordered_output_router,
};
use cordial_f1r3node_adapter::shared_ordered_output::SharedOrderedOutput;
use cordial_miners_core::types::{BlockIdentity, NodeId};
use tokio::sync::Mutex;
use tower::ServiceExt as _;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_block(tag: u8) -> BlockIdentity {
    BlockIdentity {
        content_hash: [tag; 32],
        creator: NodeId(vec![tag]),
        signature: vec![tag; 64],
    }
}

/// Build an output with a fixed past timestamp so assertions are deterministic.
fn make_output(
    blocks: Vec<BlockIdentity>,
    anchor: Option<BlockIdentity>,
) -> OrderedFinalizedOutput {
    OrderedFinalizedOutput::new(blocks, anchor, 3, 4, 812).with_timestamp(1_753_331_400_000_000_000)
}

fn make_state(
    shared: Arc<Mutex<SharedOrderedOutput>>,
    stale_threshold_ns: u128,
) -> OrderedOutputServerState {
    OrderedOutputServerState::new(shared, stale_threshold_ns)
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    serde_json::from_slice(&bytes).expect("response body should be valid JSON")
}

// ── GET /ordered-output/latest ────────────────────────────────────────────────

#[tokio::test]
async fn get_latest_returns_503_when_empty() {
    let shared = Arc::new(Mutex::new(SharedOrderedOutput::new()));
    let app = ordered_output_router(make_state(shared, 30_000_000_000));

    let req = Request::builder()
        .uri("/ordered-output/latest")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn get_latest_returns_503_when_anchor_is_none() {
    let output = make_output(vec![], None);
    let shared = Arc::new(Mutex::new(SharedOrderedOutput::from_output(output)));
    let app = ordered_output_router(make_state(shared, 30_000_000_000));

    let req = Request::builder()
        .uri("/ordered-output/latest")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn get_latest_returns_200_with_correct_fields() {
    let anchor = make_block(0);
    let output = make_output(vec![make_block(1), make_block(2)], Some(anchor.clone()));

    let shared = Arc::new(Mutex::new(SharedOrderedOutput::from_output(output)));
    let app = ordered_output_router(make_state(shared, 30_000_000_000));

    let req = Request::builder()
        .uri("/ordered-output/latest")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let json = body_json(response).await;
    assert!(json.get("blocks").is_some(), "response must include blocks");
    assert!(json.get("anchor").is_some(), "response must include anchor");
    assert_eq!(json["wavelength"], 3);
    assert_eq!(json["bond_count"], 4);
    assert_eq!(json["total_mirrored_blocks"], 812);
    assert_eq!(
        json["computed_at_ns"].as_u64().unwrap(),
        1_753_331_400_000_000_000u64
    );
    assert_eq!(json["blocks"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn get_latest_json_keys_match_schema() {
    let output = make_output(vec![make_block(1)], Some(make_block(0)));
    let shared = Arc::new(Mutex::new(SharedOrderedOutput::from_output(output)));
    let app = ordered_output_router(make_state(shared, 30_000_000_000));

    let req = Request::builder()
        .uri("/ordered-output/latest")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    let json = body_json(response).await;

    // Every documented top-level key must be present.
    for key in &[
        "blocks",
        "anchor",
        "wavelength",
        "bond_count",
        "total_mirrored_blocks",
        "computed_at_ns",
    ] {
        assert!(json.get(key).is_some(), "missing top-level key: {key}");
    }

    // Each block identity must carry the three documented sub-fields.
    let block = &json["blocks"][0];
    for key in &["content_hash", "creator", "signature"] {
        assert!(block.get(key).is_some(), "block missing key: {key}");
    }
}

// ── GET /ordered-output/status ────────────────────────────────────────────────

#[tokio::test]
async fn get_status_returns_503_when_empty() {
    let shared = Arc::new(Mutex::new(SharedOrderedOutput::new()));
    let app = ordered_output_router(make_state(shared, 30_000_000_000));

    let req = Request::builder()
        .uri("/ordered-output/status")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn get_status_returns_lightweight_object_without_block_list() {
    let output = make_output(vec![make_block(1), make_block(2)], Some(make_block(0)));
    let shared = Arc::new(Mutex::new(SharedOrderedOutput::from_output(output)));
    let app = ordered_output_router(make_state(shared, 30_000_000_000));

    let req = Request::builder()
        .uri("/ordered-output/status")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let json = body_json(response).await;

    // All documented status keys must be present.
    for key in &[
        "anchor_hash",
        "len",
        "bond_count",
        "wavelength",
        "computed_at_ns",
        "is_stale",
    ] {
        assert!(json.get(key).is_some(), "missing status key: {key}");
    }

    // The full block list must not appear in the status response.
    assert!(
        json.get("blocks").is_none(),
        "status must not include the blocks array"
    );

    assert_eq!(json["len"], 2);
    assert_eq!(json["bond_count"], 4);
    assert_eq!(json["wavelength"], 3);
}

// ── Staleness ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn is_stale_true_when_timestamp_is_old() {
    // timestamp = 0 is always older than any reasonable threshold.
    let output = make_output(vec![make_block(1)], Some(make_block(0))).with_timestamp(0);
    let shared = Arc::new(Mutex::new(SharedOrderedOutput::from_output(output)));
    let app = ordered_output_router(make_state(shared, 30_000_000_000 /* 30 s */));

    let req = Request::builder()
        .uri("/ordered-output/status")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    let json = body_json(response).await;
    assert_eq!(json["is_stale"], true);
}

#[tokio::test]
async fn is_stale_false_for_recent_timestamp() {
    let now_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let output = make_output(vec![make_block(1)], Some(make_block(0))).with_timestamp(now_ns);
    let shared = Arc::new(Mutex::new(SharedOrderedOutput::from_output(output)));
    let app = ordered_output_router(make_state(shared, 30_000_000_000 /* 30 s */));

    let req = Request::builder()
        .uri("/ordered-output/status")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    let json = body_json(response).await;
    assert_eq!(json["is_stale"], false);
}
