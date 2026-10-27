#!/usr/bin/env python3
"""Export a verified four-observer shadow snapshot for the HTML PoR demo."""

import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import runpy
import tempfile
import time


REPO = Path(__file__).resolve().parents[2]
CHECKER = runpy.run_path(str(REPO / "docker/scripts/verify-four-node-por-shadow.py"))


def snapshot(data_root, min_por_round, max_status_age_seconds=15):
    nodes = CHECKER["NODES"]
    statuses = {}
    updated_at = {}
    missing = []
    for node in nodes:
        path = CHECKER["status_path"](data_root, node)
        try:
            status = CHECKER["read_status"](path, -1)
            if status is not None:
                statuses[node] = status
                updated_at[node] = datetime.fromtimestamp(
                    path.stat().st_mtime, timezone.utc
                ).isoformat()
            else:
                missing.append(node)
        except (OSError, ValueError) as error:
            missing.append(f"{node}: {error}")

    common = None
    error = f"waiting for {', '.join(missing)}" if missing else None
    if error is None:
        try:
            common = CHECKER["compare_statuses"](statuses, 2, min_por_round)
        except (KeyError, TypeError, ValueError) as failure:
            error = str(failure)
    if not missing:
        stale = [node for node, updated in updated_at.items()
                 if (datetime.now(timezone.utc) - datetime.fromisoformat(updated)).total_seconds()
                 > max_status_age_seconds]
        if stale:
            error = f"stale observer status: {', '.join(stale)}"
    document = {
        "schema_version": 1,
        "kind": "live_shadow_snapshot",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source": str(data_root),
        "common_finalized_prefix_len": common,
        "error": error,
        "status_updated_at": updated_at,
        "statuses": statuses,
    }
    return document


def write_snapshot(output, document):
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        mode="w", encoding="utf-8", dir=output.parent,
        prefix=f".{output.name}.", delete=False,
    ) as temporary:
        temporary.write(json.dumps(document, indent=2, sort_keys=True) + "\n")
        temporary_path = Path(temporary.name)
    os.replace(temporary_path, output)


def export(data_root, output, min_por_round):
    document = snapshot(data_root, min_por_round)
    if document["error"]:
        raise ValueError(document["error"])
    write_snapshot(output, document)
    print(f"Wrote {output}: {document['common_finalized_prefix_len']} common finalized blocks")


def watch(data_root, output, min_por_round, interval):
    previous = None
    try:
        while True:
            document = snapshot(data_root, min_por_round)
            write_snapshot(output, document)
            current = document["error"] or (
                f"round {next(iter(document['statuses'].values()))['active_por_round']}, "
                f"{document['common_finalized_prefix_len']} common finalized blocks"
            )
            if current != previous:
                print(f"PoR demo feed: {current}", flush=True)
                previous = current
            time.sleep(interval)
    except KeyboardInterrupt:
        return


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data-root", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=REPO / "docs/demos/por-live-shadow.json")
    parser.add_argument("--min-por-round", type=int, default=0)
    parser.add_argument("--watch-seconds", type=float,
                        help="refresh the demo JSON continuously at this interval")
    args = parser.parse_args()
    if args.watch_seconds is not None:
        if args.watch_seconds < 1:
            parser.error("--watch-seconds must be at least 1")
        watch(args.data_root, args.output, args.min_por_round, args.watch_seconds)
    else:
        export(args.data_root, args.output, args.min_por_round)


if __name__ == "__main__":
    main()
