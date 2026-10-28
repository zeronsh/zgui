#!/usr/bin/env python3
"""Check real Liquid Glass hierarchy and live Ghostty color changes on macOS.

Build zgx-desktop examples glass_counter and ghostty_pane (feature ghostty)
first. The fixtures inspect public native API state and preserve a shell
variable across terminal color updates. No user terminal sessions are used.
"""
import argparse
import pathlib
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=pathlib.Path, default=pathlib.Path("target/debug/examples"))
    parser.add_argument("--output", type=pathlib.Path, default=pathlib.Path("/tmp/zgx-macos-appearance"))
    args = parser.parse_args()
    if sys.platform != "darwin":
        print("SKIP: AppKit and embedded Ghostty are macOS only")
        return
    args.output.mkdir(parents=True, exist_ok=True)
    for example, flag, marker in [
        ("glass_counter", "--appearance-fixture", "APPEARANCE PASS"),
        ("ghostty_pane", "--colors-fixture", "TERMINAL COLORS PASS"),
    ]:
        result = subprocess.run([str((args.bin / example).resolve()), flag], capture_output=True, text=True, timeout=30)
        log = result.stdout + result.stderr
        (args.output / (example + ".log")).write_text(log)
        assert result.returncode == 0 and marker in log, log
        print(example + ": PASS")


if __name__ == "__main__":
    main()
