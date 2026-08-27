//! HTTP server binary for ordered finalized output.
//!
//! This binary connects to a running f1r3node over gRPC, mirrors recent blocks
//! into a local Cordial blocklace, computes the finalized ordered output, and
//! serves it over a lightweight HTTP API:
//!
//! | Endpoint                        | Description                             |
//! |---------------------------------|-----------------------------------------|
//! | `GET /ordered-output/latest`    | Full `OrderedFinalizedOutput` as JSON   |
//! | `GET /ordered-output/status`    | Lightweight status (no block list)      |
//!
//! ## Usage
//!
//! ```bash
//! cargo run --bin ordered_output_server -- \
//!   --grpc-url http://127.0.0.1:40401 \
//!   --addr     127.0.0.1:7080
//! ```
//!
//! Then inspect:
//!
//! ```bash
//! curl http://127.0.0.1:7080/ordered-output/latest
//! curl http://127.0.0.1:7080/ordered-output/status
//! ```
//!
//! ## Startup sequence
//!
//! 1. Connect to f1r3node gRPC endpoint.
//! 2. Fetch `--depth` recent blocks and mirror them into a [`LiveIngress`].
//! 3. Compute initial [`OrderedFinalizedOutput`], store in [`SharedOrderedOutput`].
//! 4. Spawn background task: poll gRPC every `--poll-interval-ms` ms, re-compute
//!    ordered output, update shared state.
//! 5. Start axum listener on `--addr`.
//!
//! The server never panics on startup if no finalized output exists yet — the
//! handlers return `503` until the first finalized leader is observed.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use cordial_f1r3node_adapter::grpc_ingest::BlocklaceAdapter;
use cordial_f1r3node_adapter::live_grpc::{
    LiveGrpcBlockClient, trusted_block_from_light_block_info_with_options,
};
use cordial_f1r3node_adapter::live_ingress::LiveIngress;
use cordial_f1r3node_adapter::ordered_output_server::serve;
use cordial_f1r3node_adapter::shard_conf::CasperShardConf;
use cordial_f1r3node_adapter::shared_ordered_output::SharedOrderedOutput;
use cordial_miners_core::Block;
use cordial_miners_core::types::{BlockIdentity, NodeId};
use models::rust::string_ops::StringOps;
use tokio::sync::Mutex;

// ── CLI args ──────────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(name = "ordered-output-server")]
#[command(
    about = "Serve the latest Cordial finalized ordered output over HTTP (GET /ordered-output/latest and /ordered-output/status)"
)]
struct Args {
    /// f1r3node gRPC endpoint to mirror.
    #[arg(long, default_value = "http://127.0.0.1:40401")]
    grpc_url: String,

    /// HTTP bind address for the ordered-output API.
    #[arg(long, default_value = "127.0.0.1:7080")]
    addr: SocketAddr,

    /// Block history depth to fetch on startup.
    #[arg(long, default_value_t = 128)]
    depth: i32,

    /// Consensus wavelength for tau ordering.
    #[arg(long, default_value_t = 3)]
    wave_length: u64,

    /// How often (in milliseconds) the background task polls gRPC for new blocks.
    #[arg(long, default_value_t = 2000)]
    poll_interval_ms: u64,

    /// Shard to observe.
    #[arg(long, default_value = "root")]
    shard_id: String,

    /// Age in seconds before `/ordered-output/status` reports `is_stale: true`.
    #[arg(long, default_value_t = 30)]
    stale_threshold_secs: u64,
}

// ── Passthrough adapter ───────────────────────────────────────────────────────

/// Minimal adapter that discards block-level side-effects.
///
/// The ordered-output server only reads consensus state; it does not need to
/// write to any external store on each block ingestion.
struct PassthroughAdapter;

impl BlocklaceAdapter<BlockIdentity> for PassthroughAdapter {
    fn on_block(&mut self, _block: Block) -> anyhow::Result<()> {
        Ok(())
    }
}

// ── Startup helpers ───────────────────────────────────────────────────────────

/// Derive a uniform bond map from any block senders observed in `blocks`.
///
/// All observed validators receive an equal weight of 100. This matches the
/// approach used by the other live-node binaries and is intentionally simple
/// for a read-only mirroring tool.
fn derive_uniform_bonds(blocks: &[models::casper::LightBlockInfo]) -> HashMap<NodeId, u64> {
    let mut bonds = HashMap::new();
    for block in blocks {
        if let Some(sender) = StringOps::decode_hex(block.sender.clone()) {
            bonds.entry(NodeId(sender)).or_insert(100);
        }
    }
    bonds
}

/// Mirror `blocks` into `ingress`. Returns the number of blocks applied.
fn mirror_blocks(
    ingress: &mut LiveIngress<PassthroughAdapter>,
    blocks: &[models::casper::LightBlockInfo],
) -> Result<usize> {
    let mut applied = 0;
    for info in blocks {
        let block = trusted_block_from_light_block_info_with_options(info, true)
            .with_context(|| format!("failed to reconstruct trusted block {}", info.block_hash))?;
        ingress
            .ingest_trusted_block(block)
            .with_context(|| format!("failed to mirror live block {}", info.block_hash))?;
        applied += 1;
    }
    Ok(applied)
}

// ── main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // ── Step 1: Connect to gRPC ───────────────────────────────────────────────
    tracing_subscriber::fmt::init();

    let mut grpc = LiveGrpcBlockClient::connect(args.grpc_url.clone())
        .await
        .with_context(|| format!("failed to connect to gRPC endpoint {}", args.grpc_url))?;

    tracing::info!(grpc_url = %args.grpc_url, "connected to f1r3node gRPC");

    // ── Step 2: Fetch recent blocks and mirror into LiveIngress ───────────────
    let recent_blocks = grpc
        .recent_light_blocks(args.depth)
        .await
        .with_context(|| format!("failed to fetch recent blocks at depth {}", args.depth))?;

    tracing::info!(count = recent_blocks.len(), "fetched recent blocks");

    let bonds = derive_uniform_bonds(&recent_blocks);
    let shard_conf = CasperShardConf {
        shard_name: args.shard_id.clone(),
        max_number_of_parents: 16,
        fault_tolerance_threshold: 0.333,
        deploy_lifespan: 50,
        min_phlo_price: 1,
        ..CasperShardConf::default()
    };

    let mut ingress =
        LiveIngress::with_consensus_view(PassthroughAdapter, bonds, shard_conf, &args.shard_id);

    let applied = mirror_blocks(&mut ingress, &recent_blocks)
        .context("failed to mirror initial block set")?;
    tracing::info!(applied, "initial blocks mirrored");

    // ── Step 3: Compute initial output ────────────────────────────────────────
    let initial_output = ingress
        .latest_finalized_ordered_output(args.wave_length)
        .map_err(|err| anyhow::anyhow!("failed to compute initial ordered output: {err:?}"))?;

    let finalized_len = initial_output.len();
    tracing::info!(
        finalized_blocks = finalized_len,
        anchor = ?initial_output.anchor_hash().map(hex::encode),
        "initial ordered output computed"
    );

    // ── Step 4: Wrap in Arc<Mutex<SharedOrderedOutput>> ──────────────────────
    let shared = Arc::new(Mutex::new(SharedOrderedOutput::from_output(initial_output)));

    // ── Background polling task ───────────────────────────────────────────────
    {
        let shared = Arc::clone(&shared);
        let grpc_url = args.grpc_url.clone();
        let shard_id = args.shard_id.clone();
        let wave_length = args.wave_length;
        let poll_interval = Duration::from_millis(args.poll_interval_ms);
        let depth = args.depth;

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(poll_interval).await;

                let result: Result<()> = async {
                    let mut grpc = LiveGrpcBlockClient::connect(grpc_url.clone())
                        .await
                        .context("reconnect failed")?;

                    let blocks = grpc
                        .recent_light_blocks(depth)
                        .await
                        .context("failed to fetch recent blocks")?;

                    let bonds = derive_uniform_bonds(&blocks);
                    let shard_conf = CasperShardConf {
                        shard_name: shard_id.clone(),
                        max_number_of_parents: 16,
                        fault_tolerance_threshold: 0.333,
                        deploy_lifespan: 50,
                        min_phlo_price: 1,
                        ..CasperShardConf::default()
                    };

                    let mut ingress = LiveIngress::with_consensus_view(
                        PassthroughAdapter,
                        bonds,
                        shard_conf,
                        &shard_id,
                    );

                    mirror_blocks(&mut ingress, &blocks).context("mirror failed")?;

                    let output = ingress
                        .latest_finalized_ordered_output(wave_length)
                        .map_err(|err| anyhow::anyhow!("ordering failed: {err:?}"))?;

                    let len = output.len();
                    let mut guard = shared.lock().await;
                    match guard.update(output) {
                        Ok(()) => {
                            tracing::debug!(finalized_blocks = len, "ordered output updated");
                        }
                        Err(err) => {
                            // Prefix violation: the new output regressed.
                            // Log and skip — we prefer a stale-but-correct output
                            // over a regressed one.
                            tracing::warn!(
                                error = %err,
                                "ordered output update rejected (prefix violation) — keeping previous"
                            );
                        }
                    }
                    Ok(())
                }
                .await;

                if let Err(err) = result {
                    tracing::error!(error = %err, "background poll cycle failed");
                }
            }
        });
    }

    // ── Step 5: Start HTTP server ─────────────────────────────────────────────
    let stale_threshold_ns = (args.stale_threshold_secs as u128) * 1_000_000_000;
    tracing::info!(addr = %args.addr, "starting ordered-output HTTP server");

    serve(shared, args.addr, stale_threshold_ns)
        .await
        .with_context(|| format!("HTTP server on {} failed", args.addr))?;

    Ok(())
}
