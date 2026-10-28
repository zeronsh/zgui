#!/usr/bin/env python3
"""Operator probe for application shortcuts while a macOS WebKit page has focus.

Start an inspected zterm with an isolated session, open a browser page, then
run this script with its PID. It does not send input or change window focus.
"""

import argparse
import json
import subprocess
import sys
import time
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", help="PID shown by zgx inspect list")
    parser.add_argument("--zgx", type=Path, default=Path("target/debug/zgx"))
    parser.add_argument("--timeout", type=float, default=30)
    args = parser.parse_args()
    if sys.platform != "darwin":
        print("SKIP: this probe requires macOS and WebKit")
        return
    if not args.app:
        parser.error("--app is required on macOS")

    def palette():
        result = subprocess.run(
            [str(args.zgx), "inspect", "state", "state.luau::palette", "--app", args.app, "--json"],
            capture_output=True,
            text=True,
            check=True,
        )
        data = json.loads(result.stdout)
        if not data.get("ok"):
            raise RuntimeError(data)
        return data["data"]["state.luau::palette"]

    if palette() is not False:
        raise RuntimeError("close the command palette before starting the probe")
    print("Focus a loaded browser page in zterm, then press Cmd-K.", flush=True)
    deadline = time.monotonic() + args.timeout
    while time.monotonic() < deadline:
        if palette() is True:
            print("PASS: Cmd-K opened the palette; confirm the browser page had focus.")
            return
        time.sleep(0.1)
    raise TimeoutError("palette did not open; confirm the browser was focused and retry")


if __name__ == "__main__":
    main()
