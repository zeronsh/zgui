#!/usr/bin/env python3
"""Compare archived/current TextEditor modules in identical small release builds."""
import argparse
import datetime
import sys
import hashlib
import json
import os
import pathlib
import platform
import statistics
import subprocess
import tarfile
import tempfile
import tomllib


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=pathlib.Path, default=pathlib.Path("docs/platform-validation/initialization/source.tar.gz"))
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--repeats", type=int, default=5)
    args = parser.parse_args()
    assert 1 <= args.repeats <= 20
    root = pathlib.Path(__file__).resolve().parents[1]
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / "results.json").unlink(missing_ok=True)
    source_path = "crates/zgx/src/text_edit.rs"
    with tarfile.open(args.archive) as archive:
        before = archive.extractfile(source_path).read()
    after = (root / source_path).read_bytes()
    example = (root / "crates/zgx/examples/editor_streaming.rs").read_bytes()
    lock = tomllib.loads((root / "Cargo.lock").read_text())
    unicode_version = next(p["version"] for p in lock["package"] if p["name"] == "unicode-segmentation")
    manifest = '[package]\nname="zgx"\nversion="0.1.0"\nedition="2024"\n[dependencies]\nunicode-segmentation="=' + unicode_version + '"\n[profile.release]\nlto="thin"\ncodegen-units=1\n'
    proof = dict(archive=str(args.archive), archive_sha256=sha(args.archive.read_bytes()), before_sha256=sha(before), after_sha256=sha(after), benchmark_sha256=sha(example), unicode_version=unicode_version, repeats=args.repeats, scope="Exact TextEditor modules only; headless external set_text, not GUI/framework comparison")
    proof["started_at_utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    proof["command"] = [sys.executable, *sys.argv]
    proof["build_command"] = ["cargo", "build", "--release", "--offline"]
    proof["build_environment"] = {"CARGO_INCREMENTAL": "0", "CARGO_BUILD_JOBS": "1", "CARGO_TARGET_DIR": "per-variant temporary target directory"}
    proof["rustc"] = subprocess.check_output(["rustc", "-Vv"], text=True)
    proof["cargo"] = subprocess.check_output(["cargo", "-V"], text=True).strip()
    proof["platform"] = platform.platform()
    runner = pathlib.Path(__file__).read_bytes()
    proof["runner_sha256"] = sha(runner)
    (args.output / "run.py").write_bytes(runner)
    with tempfile.TemporaryDirectory(prefix="zgx-editor-streaming-") as temporary:
        build_root = pathlib.Path(temporary)
        binaries = {}
        env = dict(os.environ, CARGO_INCREMENTAL="0", CARGO_BUILD_JOBS="1")
        for name, source in (("before", before), ("after", after)):
            work = build_root / name
            (work / "src").mkdir(parents=True)
            (work / "Cargo.toml").write_text(manifest)
            (work / "src/lib.rs").write_text("pub mod text_edit;\n")
            (work / "src/text_edit.rs").write_bytes(source)
            (work / "src/main.rs").write_bytes(example)
            env["CARGO_TARGET_DIR"] = str(work / "target")
            with (args.output / (name + "-build.log")).open("w") as log:
                subprocess.run(["cargo", "build", "--release", "--offline"], cwd=work, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
            binary = work / "target/release/zgx"
            binaries[name] = binary
            proof[name + "_binary_sha256"] = sha(binary.read_bytes())
            (args.output / (name + "-text_edit.rs")).write_bytes(source)
            (args.output / (name + "-Cargo.lock")).write_bytes((work / "Cargo.lock").read_bytes())
        rows = []
        for repeat in range(args.repeats):
            for name in (("before", "after") if repeat % 2 == 0 else ("after", "before")):
                row = json.loads(subprocess.check_output([str(binaries[name])], text=True))
                assert row["correct"] and row["undo_steps"] == row["redo_steps"]
                rows.append(dict(variant=name, repeat=repeat, **row))
        summary = {}
        for name in binaries:
            group = [r for r in rows if r["variant"] == name]
            summary[name] = {field: dict(median=statistics.median(r[field] for r in group), minimum=min(r[field] for r in group), maximum=max(r[field] for r in group)) for field in ("elapsed_ms", "rss_before_kib", "rss_after_kib", "peak_rss_kib", "requested_live_before_bytes", "requested_live_after_bytes", "requested_peak_bytes", "undo_steps") if all(r[field] is not None for r in group)}
        assert (root / source_path).read_bytes() == after, "current TextEditor changed during measurement"
        assert (root / "crates/zgx/examples/editor_streaming.rs").read_bytes() == example, "benchmark changed during measurement"
        assert pathlib.Path(__file__).read_bytes() == runner, "runner changed during measurement"
        assert sha(args.archive.read_bytes()) == proof["archive_sha256"], "baseline archive changed"
        proof["finished_at_utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        (args.output / "benchmark.rs").write_bytes(example)
        (args.output / "Cargo.toml").write_text(manifest)
        (args.output / "metadata.json").write_text(json.dumps(proof, indent=2) + "\n")
        (args.output / "results.json").write_text(json.dumps(dict(rows=rows, summary=summary), indent=2) + "\n")
        print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
