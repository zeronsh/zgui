#!/usr/bin/env python3
"""Write Direct3D 11's shaders again from their WGSL, and print their hashes.

naga translates draw.wgsl, copy_offset.wgsl and polyline.wgsl to HLSL
(crates/zgx-gpu/src/*.hlsl), and FXC compiles that to the DXBC in
crates/zgx-gpu/src/dxbc/, as wgpu's Direct3D 12 backend compiles its own.
FXC runs on Windows alone, so this does too:

    python scripts/d3d11_shaders.py          # write them, print sha256s
    python scripts/d3d11_shaders.py --check  # only check they are current

The test it runs, `translated_hlsl_matches` (d3d11_frame.rs), fails when
any is stale. The hashes let a machine without FXC take files written here
and check they arrived whole.
"""
import argparse
import hashlib
import os
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SRC = ROOT / "crates" / "zgx-gpu" / "src"


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--check", action="store_true", help="fail if any is stale, writing nothing")
    args = parser.parse_args()
    if os.name != "nt":
        sys.exit("FXC runs on Windows alone: run this there")
    env = dict(os.environ)
    if args.check:
        env.pop("ZGX_BLESS", None)
    else:
        env["ZGX_BLESS"] = "1"
    command = ["cargo", "test", "-p", "zgx-gpu", "--features", "d3d11", "--lib", "--locked",
               "translated_hlsl"]
    result = subprocess.run(command, cwd=ROOT, env=env)
    if result.returncode != 0:
        sys.exit(result.returncode)
    for path in sorted([*SRC.glob("*.hlsl"), *(SRC / "dxbc").glob("*.dxbc")]):
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        print(f"{digest}  {path.relative_to(ROOT).as_posix()}  {path.stat().st_size} B")


if __name__ == "__main__":
    main()
