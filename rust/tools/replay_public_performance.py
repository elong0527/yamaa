"""Stream one original Actions ZIP at a time into the independent auditor.

This is a network/storage adapter only. The auditor selects, extracts, compares
and evaluates the immutable raw evidence; the adapter never admits a sample.
"""

from __future__ import annotations

import json
import tempfile
from contextlib import contextmanager
from pathlib import Path

from retain_public_performance import download


@contextmanager
def original_archive(entry, manifest_root):
    manifest = json.loads((manifest_root / "manifest.json").read_text())
    # Missing identity remains absent and is reported by the auditor before
    # attempting retrieval; no other tuple or prior attempt is substituted.
    with tempfile.TemporaryDirectory(prefix="yamaa-original-performance-") as temporary:
        root = Path(temporary)
        archive = entry["archive"]
        if Path(archive).name != archive:
            raise ValueError("performance archive must have a simple relative name")
        if type(entry["artifact_id"]) is int and entry["artifact_id"] > 0:
            download(
                manifest["repository"],
                f"actions/artifacts/{entry['artifact_id']}/zip",
                root / archive,
            )
        yield root


if __name__ == "__main__":
    import audit_public_performance

    raise SystemExit(audit_public_performance.main(entry_provider=original_archive))
