#!/usr/bin/env python3
"""Bounded native component-host lifecycle smoke; requires a desktop session.

This verifies model progress and clean window shutdown, not presentation cadence
or native input. GPU pixel correctness is exercised separately by zgx-gpu tests.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    results = []
    for mode in ("idle", "both"):
        env = dict(os.environ, ZGX_MODE=mode, ZGX_SECONDS="2")
        env.pop("ZGX_INITIAL_TICKS", None)
        with (args.output / f"{mode}.log").open("w") as log:
            process = subprocess.run(
                [str(args.binary.resolve())], env=env, stdout=log,
                stderr=subprocess.STDOUT, timeout=45, check=False,
            )
        lines = (args.output / f"{mode}.log").read_text().splitlines()
        records = [json.loads(line) for line in lines if line.startswith('{"framework":')]
        assert process.returncode == 0, (mode, process.returncode)
        assert len(records) == 1, (mode, records)
        record = records[0]
        assert record["framework"] == "zgx" and record["renderer"] == "gpu", record
        assert record["adapter"] == "components", record
        if mode == "idle":
            assert record["ticks"] == 0, record
        else:
            assert record["ticks"] > 0, record
        assert 1 <= record["mounted_rows"] <= 32, record
        assert 1.5 <= record["elapsed_seconds"] < 45, record
        results.append(dict(mode=mode, **record))
    (args.output / "results.json").write_text(json.dumps(results, indent=2) + "\n")
    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
