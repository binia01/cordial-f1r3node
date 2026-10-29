# PoR observatory

The [dashboard](por-weighted-slashing.html) is a local, buildless simulation
explorer. It uses no external fonts, chart libraries, analytics, or uploads.
The interface keeps three evidence sources separate:

- **Guided example:** the checked-in two-round fixture. It shows exact recorded
  scores, isolated slashing, retained capital, exclusion, and recovery. It is
  historical evidence, not a result of the newly added tests.
- **Imported scenarios:** JSON exported by the multi-round Rust runner. Honest,
  unequal-start, isolated-fault, and correlated-fault files can be imported
  together. Empty scenarios never receive invented trajectories.
- **Live observers:** optional shadow status snapshots. Freshness, agreement,
  catch-up, and conflicts are reported independently of imported results.

## Open locally

From the repository root:

    python3 -m http.server 8765 --bind 127.0.0.1 --directory docs

Open [the observatory](http://127.0.0.1:8765/demos/por-weighted-slashing.html).
Serving the docs root also makes the linked run instructions available.
The dashboard does not launch tests or a cluster.

If the server runs on a remote host, forward the loopback port from your laptop:

    ssh -N -o ExitOnForwardFailure=yes -L 8876:127.0.0.1:8765 USER@REMOTE_NODE

Then open http://127.0.0.1:8876/demos/por-weighted-slashing.html.

## Explore a run

1. Choose **Import results** and select one or more JSON files, or drag files into
   the import dialog. Files stay in memory and are cleared on refresh.
2. Choose a loaded scenario. Each card shows its number of recorded rounds.
3. Switch between **Reputation**, **Voting share**, and **Stability**.
4. Scrub the timeline, click the chart, or play recorded rounds at 1x, 2x, or 5x.
   **Fault** jumps to the first recorded exclusion. The inspector explains the
   selected checkpoint; the ledger shows exact values.
5. Inspect the checkpoint hash and, in the saved example, the signed proof
   identities and penalty commitment.

Arrow keys step through rounds and Space toggles playback while the chart is in
view and an input is not focused. Playback pauses when the tab becomes hidden.
The layout supports narrow screens; the exact-value table scrolls horizontally
when necessary.

Stability charts divide score and voting-share deltas by the file's acceptance
bounds. The vertical axis is logarithmic; 1x is the bound and 0 is no change.
Ranking must also remain stable for the recorded acceptance window.
A line connects recorded samples; it does not create intermediate observations.

## Export results (run manually)

    POR_CONVERGENCE_OUTPUT=/tmp/por-convergence cargo test -j 1 -p cordial-f1r3node-adapter --test test_por_checkpoint_penalties multi_round_ranking_and_slashing_convergence -- --exact

Import honest.json, unequal.json, isolated.json, and correlated.json.
The versioned format has kind "por_convergence_trace", scenario name,
observer count, thresholds, completion status, and a rounds array.
Each round includes active weights, checkpoint identity, measured deltas,
ranking stability, observer agreement, events, and explicitly labeled
reputation entries. Restart markers identify the first record after recovery.

The runner writes a scenario file only after its assertions finish successfully.
A missing file is not a failure report or a passing result. The UI labels
completion status as **reported by the file**; importing JSON is not an audit
or a new test execution.

Earlier array-only exports remain readable. If those entries lack validator
labels, retained balances and exclusions are left unknown rather than guessed
from public-key order. Files are limited to 5 MB and 10,000 rounds.

To regenerate the historical example after changing its fixture:

    bash docker/scripts/build-por-demo-fixture.sh

See [convergence validation](../por-convergence-validation.md) for the complete
adapter, ordering, penalty, and restart commands.

## Optional live feed

Start the isolated cluster and keyed shadow observers using the
[shadow runner guide](../por-shadow-runner.md). The smoke verifier stops its
observer processes when it exits; keep or restart those observers if you want
a continuously updating dashboard.

    python3 docker/scripts/export-por-demo-live.py --data-root PATH --min-por-round 4 --watch-seconds 2

Use the actual observer data root. This exports
docs/demos/por-live-shadow.json; the dashboard polls it every 2.5 seconds.
The exporter does not start or stop validators.

Different active rounds are labeled **Catching up**. Conflicting finalized
prefixes or different projections at the same round are labeled **Reports
conflict**. Old snapshots remain distinguishable from live evidence. The
progress line only reports an advance after the common finalized prefix
actually grows during the browser session.

Neither imported reputation traces nor shadow agreement establish live
f1r3node validator-side weight enforcement or host-chain slash execution.
