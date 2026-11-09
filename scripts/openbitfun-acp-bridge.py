#!/usr/bin/env python3
"""Transparent stdio ACP process bridge for Craft Anything -> OpenBitFun.

Craft handles permission classification and approval. This bridge deliberately
does not rewrite, approve, reject, or reinterpret ACP permission requests.
"""

import json
import os
import shutil
import subprocess
import sys
import threading
from pathlib import Path

LOG_PREFIX = "[openbitfun-acp-bridge]"


def log(message):
    print(f"{LOG_PREFIX} {message}", file=sys.stderr, flush=True)


def resolve_executable(program):
    """Resolve an explicit path or a command installed on PATH."""
    candidate = Path(program).expanduser()

    if candidate.is_file():
        return str(candidate.resolve())

    resolved = shutil.which(program)
    if resolved:
        return resolved

    raise FileNotFoundError(
        f"OpenBitFun executable '{program}' was not found. "
        "Set the provider executable path or add OpenBitFun to PATH."
    )


def forward_client_input(child):
    """Relay Craft's stdin to the ACP child without modifying messages."""
    if child.stdin is None:
        return

    try:
        while True:
            line = sys.stdin.buffer.readline()
            if not line:
                break

            child.stdin.write(line)
            child.stdin.flush()

    except (BrokenPipeError, OSError, ValueError) as exc:
        log(f"Client input forwarding stopped: {exc}")

    finally:
        try:
            child.stdin.close()
        except (BrokenPipeError, OSError, ValueError):
            pass


def main():
    if len(sys.argv) < 2:
        log("Usage: python openbitfun-acp-bridge.py <openbitfun-exe> [acp args...]")
        return 2

    try:
        executable = resolve_executable(sys.argv[1])
    except (OSError, ValueError) as exc:
        log(str(exc))
        return 2

    command = [executable, *sys.argv[2:]]
    log(f"Starting ACP process: {executable}")

    try:
        child = subprocess.Popen(
            command,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=None,
            bufsize=0,
        )
    except OSError as exc:
        log(f"Could not start OpenBitFun: {exc}")
        return 2

    input_thread = threading.Thread(
        target=forward_client_input,
        args=(child,),
        name="openbitfun-acp-client-input",
        daemon=True,
    )
    input_thread.start()

    try:
        assert child.stdout is not None

        while True:
            line = child.stdout.readline()
            if not line:
                break

            if not line.strip():
                continue

            try:
                message = json.loads(line.decode("utf-8-sig"))
            except (UnicodeDecodeError, json.JSONDecodeError):
                # ACP stdout is a JSON-RPC transport. Never inject startup logs
                # or malformed output into Craft's protocol stream.
                log(f"Discarded non-JSON stdout line ({len(line)} bytes)")
                continue

            if not isinstance(message, dict) or message.get("jsonrpc") != "2.0":
                log(f"Discarded non-JSON-RPC stdout line ({len(line)} bytes)")
                continue

            # Forward the exact original bytes, preserving fields, IDs and options.
            sys.stdout.buffer.write(line)
            sys.stdout.buffer.flush()

    except KeyboardInterrupt:
        log("Interrupted")
    except (OSError, ValueError) as exc:
        log(f"ACP output forwarding stopped: {exc}")

    finally:
        if child.poll() is None:
            try:
                child.terminate()
            except OSError:
                pass

    try:
        return child.wait(timeout=5)
    except subprocess.TimeoutExpired:
        log("ACP child did not exit; terminating it")
        child.kill()
        return child.wait()


if __name__ == "__main__":
    raise SystemExit(main())
