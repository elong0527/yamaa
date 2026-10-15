"""Bind the #2288 coverage manifest to retained original Actions metadata."""

import argparse
from pathlib import Path

import audit_public_performance as auditor


def prepare(root, repository, revision, source_bundle=None):
    payload = {
        "protocol": auditor.PROTOCOL,
        "repository": repository,
        "source_revision": revision,
        "collection_scope": auditor.collector.LINUX_INSTALLED,
        "run": "run.json",
        "jobs": "jobs.json",
        "artifacts": "artifacts.json",
        "native_run": "native-run.json",
        "native_jobs": "native-jobs.json",
        "native_artifacts": "native-artifacts.json",
        "entries": [],
    }
    if source_bundle is not None:
        auditor.relative(root, source_bundle)
        payload["source_bundle"] = source_bundle
    run = auditor.read(root / payload["run"])
    jobs = auditor.read(root / payload["jobs"])
    jobs = jobs["jobs"] if isinstance(jobs, dict) else jobs
    artifacts = auditor.read(root / payload["artifacts"])
    artifacts = artifacts["artifacts"] if isinstance(artifacts, dict) else artifacts
    auditor.validate_run(run, payload)
    native = auditor.native_inputs(payload, root, run)
    auditor.require(
        native["status"] == "pass",
        native.get("reason", "native source admission failed"),
    )
    for lane in auditor.tuples(auditor.collector.LINUX_INSTALLED):
        name = (
            "performance-python-ubuntu-24.04-3.12-wheel"
            if lane["host"] == "python"
            else "performance-r-ubuntu-24.04-source"
        )
        selected = []
        for artifact in artifacts:
            if artifact["name"] != name:
                continue
            entry = {
                "tuple": lane,
                "artifact_id": artifact["id"],
                "artifact_name": name,
                "run_attempt": run["run_attempt"],
                "campaign": name + "/machine-campaign.json",
                "collections": [
                    {"role": "candidate", "path": name + "/candidate-evidence"}
                ],
            }
            try:
                auditor.admit_artifact(entry, artifacts, run, jobs)
            except auditor.EVIDENCE_ERRORS:
                continue
            selected.append(entry)
        auditor.require(
            len(selected) == 1, "missing/ambiguous original producing artifact: " + name
        )
        payload["entries"].extend(selected)
    return payload


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--source-bundle")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    payload = prepare(
        args.root.resolve(), args.repository, args.revision, args.source_bundle
    )
    with args.output.open("x", encoding="utf-8") as stream:
        import json

        json.dump(payload, stream, indent=2)
        stream.write("\n")


if __name__ == "__main__":
    main()
