"""Compare core float rendering to Python's canonical positional representation.

This differential check never creates or changes expected fixtures. Independent
halfway cases and extreme exponents remain in the Rust/Python conversion tests.
"""

import decimal
import math
import struct
import subprocess
from pathlib import Path

WORKSPACE = Path(__file__).resolve().parents[1]


def main():
    """Replay the deterministic Rust sample and fail on any differing digit or sign."""
    output = subprocess.check_output(
        [
            "cargo",
            "run",
            "--offline",
            "--quiet",
            "-p",
            "yamaa-core",
            "--example",
            "float_text_sample",
        ],
        cwd=WORKSPACE,
        text=True,
    )
    count = 0
    for line in output.splitlines():
        bits, actual = line.split("\t")
        value = struct.unpack(">d", bytes.fromhex(bits))[0]
        if not math.isfinite(value):
            raise AssertionError(f"non-finite sample input {bits}")
        expected = format(decimal.Decimal(repr(value)), "f").removesuffix(".0")
        if actual != expected:
            raise AssertionError(
                f"{bits}: core {actual!r} differs from Python {expected!r}"
            )
        count += 1
    if count < 19000:
        raise AssertionError(f"incomplete finite sample: {count} values")
    print(f"Canonical float text matches Python for {count} finite bit patterns")


if __name__ == "__main__":
    main()
