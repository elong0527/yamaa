"""Compare core SQRT with Python math.sqrt by exact bits, without writing fixtures."""

import math
import struct
import subprocess
from pathlib import Path

WORKSPACE = Path(__file__).resolve().parents[1]


def main():
    """Check signed zero, every exponent, boundary mantissas and deterministic samples."""
    output = subprocess.check_output(
        [
            "cargo",
            "run",
            "--offline",
            "--quiet",
            "-p",
            "yamaa-core",
            "--example",
            "sqrt_sample",
        ],
        cwd=WORKSPACE,
        text=True,
    )
    count = 0
    for line in output.splitlines():
        bits, actual = line.split("\t")
        value = struct.unpack(">d", bytes.fromhex(bits))[0]
        assert math.isfinite(value) and value >= 0.0
        expected = struct.pack(">d", math.sqrt(value)).hex()
        if actual != expected:
            raise AssertionError(
                f"{bits}: core {actual} differs from Python {expected}"
            )
        count += 1
    if count < 110000:
        raise AssertionError(f"incomplete finite sample: {count} values")
    print(f"SQRT matches Python exactly for {count} finite binary64 inputs")


if __name__ == "__main__":
    main()
