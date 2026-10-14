"""Retain direct machine/campaign identity for same-runner performance evidence.

Create once on the orchestrating machine, then attach after each collection on
that same machine. This adds provenance only; no failed or diagnostic sample is
admitted by an attachment, and no candidate/baseline scopes are equated.
"""

from __future__ import annotations

import argparse
import datetime
import json
import os
import platform
import uuid
from pathlib import Path

PROTOCOL = "public-performance-campaign/1"


def machine():
    result = {
        "node": platform.node(),
        "system": platform.system(),
        "architecture": platform.machine(),
        "kernel": platform.release(),
        "processor": platform.processor() or None,
        "logical_cpus": os.cpu_count(),
        "runner_name": os.environ.get("RUNNER_NAME"),
        "runner_image": os.environ.get("ImageOS"),
        "runner_image_version": os.environ.get("ImageVersion"),
        "cpu_quota": None,
    }
    if result["system"] == "Linux":
        quota = Path("/sys/fs/cgroup/cpu.max")
        if quota.is_file():
            result["cpu_quota"] = quota.read_text().strip()
    if not result["node"] or not result["architecture"]:
        raise ValueError("actual machine node or architecture is unavailable")
    return result


def read(path):
    return json.loads(path.read_text(encoding="utf-8"))


def write(path, value):
    path.write_text(
        json.dumps(value, indent=2, ensure_ascii=True) + "\n", encoding="utf-8"
    )


def create(path):
    value = {
        "protocol": PROTOCOL,
        "campaign_id": str(uuid.uuid4()),
        "created_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "measurement_machine": machine(),
        "collections": [],
    }
    # An existing campaign must never be silently replaced between two owners.
    with path.open("x", encoding="utf-8") as output:
        output.write(json.dumps(value, indent=2, ensure_ascii=True) + "\n")
    return value


def attach(path, directory, role):
    campaign = read(path)
    if (
        campaign.get("protocol") != PROTOCOL
        or str(uuid.UUID(campaign["campaign_id"])) != campaign["campaign_id"]
        or campaign["measurement_machine"] != machine()
    ):
        raise ValueError("performance campaign belongs to another actual machine")
    collection_path = directory.resolve() / "collection.json"
    value = read(collection_path)
    if (
        value.get("protocol") != "public-performance/1"
        or value["provenance"]["system"] != campaign["measurement_machine"]["system"]
    ):
        raise ValueError("collection system/protocol differs from actual campaign")
    receipt = {
        "protocol": PROTOCOL,
        "campaign_id": campaign["campaign_id"],
        "measurement_machine": campaign["measurement_machine"],
        "campaign_receipt": str(path.resolve()),
        "role": role,
        "source_revision": value["source_revision"],
        "collection": str(collection_path),
        "observation": "direct machine identity observed by the orchestrating runner; equality checked again after local child collection",
    }
    previous = value.get("measurement_campaign")
    if previous is not None and previous != receipt:
        raise ValueError("cannot replace an original measurement campaign")
    value["measurement_campaign"] = receipt
    if receipt not in campaign["collections"]:
        campaign["collections"].append(receipt)
    write(collection_path, value)
    write(path, campaign)
    return receipt


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("create", "attach"))
    parser.add_argument("--campaign", type=Path, required=True)
    parser.add_argument("--collection", type=Path)
    parser.add_argument(
        "--role", choices=("candidate", "baseline", "producer-root-build-baseline")
    )
    args = parser.parse_args()
    if args.command == "create":
        if args.collection or args.role:
            parser.error("create does not attach a collection")
        create(args.campaign)
    else:
        if not args.collection or not args.role:
            parser.error("attach requires the original collection and its role")
        attach(args.campaign, args.collection, args.role)
