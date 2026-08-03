"""Emit one fixture's serialized output as JSON, for the R parity harness."""
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from runner import Spec  # noqa: E402

if __name__ == "__main__":
    json.dump(Spec(sys.argv[1]).run(), sys.stdout)
