# PoR weighted convergence and equivocation demo

The [HTML dashboard](./por-weighted-slashing.html) opens with the live observer
network, followed by a guided equivocation replay. It has two evidence sources:

1. An optional feed from four **live shadow observers**: the exporter reads the
   observers' status files, checks the common finalized prefix, active PoR round,
   checkpoint, exact weight map, and weight commitment, then refreshes a JSON
   document that the browser polls every 2.5 seconds. The dashboard labels old
   status files as a saved snapshot, even when the exporter is still running.
2. A reproducible Rust fixture: four independent durable PoR replays use a
   complete signed checkpoint rating batch, a finalized carrier containing
   verified equivocation proof, and a second checkpoint after restart. The test
   asserts the same checkpoint hashes and weights on all four stores, unequal
   weights, the offender's zero active weight, retained reputation, and
   exclusion of the ejected rater in the next round.

The controlled fixture JSON is already checked in. From the repository root,
serve the dashboard without building Rust:

```sh
python3 -m http.server 8765 --bind 127.0.0.1 --directory docs/demos
```

Open `http://127.0.0.1:8765/por-weighted-slashing.html`. The fixture loads
automatically.

For a presentation, start on the **live network** panel. The top status and
source freshness cards say whether the four reports are current, saved, stale,
or unavailable. Then select **Controlled replay** and use **Play sequence** or
the step buttons to show signed fork → finalized authorization → penalty →
restart → next round. When the replay section is on screen, Left/Right arrows
step through it and Space plays or pauses. The controlled replay is available
even with no live feed; its banner states that its slashing evidence is a
fixture. Browser refresh resets the short live trend and activity list.

If the server runs on a remote node and your browser runs on your laptop,
forward it from a **terminal on the laptop**, not a VS Code terminal attached
to the remote node. Use local port 8876 to avoid a collision with VS Code's own
port forwarding:

```sh
ssh -N -o ExitOnForwardFailure=yes -L 8876:127.0.0.1:8765 USER@REMOTE_NODE
```

Then enter `http://127.0.0.1:8876/por-weighted-slashing.html` in the laptop
browser's address bar. Use the same SSH host or alias you normally use for the
node. In VS Code Remote SSH, forwarding port 8765 in the **Ports** panel works
the same way. Copy the exact **Forwarded Address** shown for remote port 8765;
VS Code may choose a different local port. Add `/por-weighted-slashing.html` to
that address and use **Open in Browser** from the Ports panel. Opening the HTML
file in an editor preview can show source without running the JavaScript that
loads the fixture and live feed. A browser without port forwarding connects to
its own localhost.

Rebuild the fixture only after changing the Rust fixture test:

```sh
./docker/scripts/build-por-demo-fixture.sh
```

If opening the HTML directly from disk, use **Load fixture JSON** and select
`docs/demos/por-weighted-slashing.json`.

For the live panel, first run the keyed four-node checkpoint smoke described
in [the shadow runner guide](../por-shadow-runner.md#parallel-keyed-por-smoke).
Keep its data root. The smoke harness stops its observers when it exits. To
keep the panel live after the smoke passes, run four observers with the same
durable directories, keys, and rating exchange in one terminal. Set
`smoke_root` and `keys_dir` to the values used for the smoke run:

```bash
(
  pids=()
  trap 'kill "${pids[@]}" 2>/dev/null' EXIT
  for i in 1 2 3 4; do
    node="cordial-validator-$i"
    port=$((60001 + i * 1000))
    target/release/live_por_shadow \
      --grpc-url "http://127.0.0.1:$port" \
      --bonds-file docker/genesis/cordial-bonds.txt \
      --data-dir "$smoke_root/observers/$node" \
      --validator-rating-key-file "$keys_dir/validator-$i.key" \
      --stage-local-ratings --rating-exchange-dir "$smoke_root/exchange" \
      --checkpoint-ratings --commit-complete-ratings \
      --checkpoint-epoch-delay-waves 12 \
      >>"$smoke_root/observers/$node/runner.log" 2>&1 &
    pids+=("$!")
  done
  wait
)
```

In another terminal, export a continuously refreshed feed:

```sh
python3 docker/scripts/export-por-demo-live.py \
  --data-root "$HOME/cordial-por-four-node" \
  --min-por-round 1 \
  --watch-seconds 2
```

Use the actual `--data-root` passed to the smoke harness. The browser updates
without a manual refresh. Stop the exporter with Ctrl+C. For one saved export,
omit `--watch-seconds`; the page will label it as a saved snapshot when its
source files are no longer fresh. The exporter reports an explicit stale status
if an observer stops publishing for 15 seconds, even if the JSON export itself
continues to refresh. It does not start, stop, or restart validators or
observers. The live panel reports active weights only; a deferred candidate is
labeled separately.

The deterministic fixture establishes PoR penalty replay and observer
convergence under controlled finalized evidence. The live snapshot establishes
agreement among shadow observers for a running cluster. The current harness
does not inject equivocation into the running cluster, execute a host-chain
slash deploy, or prove that the f1r3node validators enforce the PoR weights.
Those are separate integration gates before claiming live weighted-validator
convergence under attack.
