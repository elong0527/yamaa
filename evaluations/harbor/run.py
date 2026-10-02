"""Run a local evaluation job with task evidence saved before execution.

    python evaluations/harbor/run.py -c <generated-job.json>

The job owns a copy of each task, including its metadata. Later builds cannot
change its inputs or provenance, even when setup fails before verification.
Completed model jobs are uploaded privately to yamaa on Harbor Hub. After
the task versions, dataset links, and archives are confirmed, local artifacts
are removed and completion metadata remains.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import shutil
import sys
from datetime import UTC, datetime
from pathlib import Path
from uuid import UUID

import tomllib


def is_model_job(config: dict) -> bool:
    agents = config.get("agents", [])
    return bool(agents) and all(
        agent.get("name") not in (None, "oracle", "nop") for agent in agents
    )


async def confirm_uploaded_job(uploader, receipt: dict) -> None:
    """Require the finalized job archive and every declared trial on the Hub."""
    job_id = UUID(receipt["job_id"])
    remote, trials = await asyncio.gather(
        uploader.db.get_job(job_id), uploader.db.list_trials_for_job(job_id)
    )
    if (
        not remote
        or not remote.get("archive_path")
        or not remote.get("finished_at")
        or remote.get("n_planned_trials") != receipt["expected_trials"]
        or len(trials) != receipt["expected_trials"]
        or {t["id"] for t in trials} != set(receipt["trial_ids"])
        or any(not t.get("archive_path") for t in trials)
    ):
        raise RuntimeError(
            "Harbor Hub has not confirmed the complete job and trial archives"
        )
    from hub import link_job_datasets

    await link_job_datasets(
        str(job_id), receipt["datasets"], receipt["expected_trials"]
    )
    receipt.update(
        verified_at=datetime.now(UTC).isoformat(),
        dataset_verified_at=datetime.now(UTC).isoformat(),
        archive_path=remote["archive_path"],
    )


def save_upload_receipt(job_dir: Path, receipt: dict) -> None:
    temporary = job_dir / "hub-upload.json.tmp"
    temporary.write_text(json.dumps(receipt, indent=2) + "\n")
    temporary.replace(job_dir / "hub-upload.json")


def cleanup_uploaded_job(job_dir: Path, receipt: dict) -> None:
    """Remove job artifacts while retaining the supervisor's completion records."""
    if (
        receipt.get("status") != "uploaded"
        or not receipt.get("verified_at")
        or not receipt.get("dataset_verified_at")
        or not receipt.get("datasets")
    ):
        raise ValueError("local cleanup requires a confirmed Harbor Hub upload")
    names = receipt["trial_directories"]
    if any(
        not name or name in (".", "..") or Path(name).name != name for name in names
    ):
        raise ValueError("trial directories must be inside the job directory")
    for name in [
        *names,
        "task-snapshots",
        ".harbor-upload",
        "hub-datasets",
        "job.log",
        "lock.json",
        "evaluation.json",
        "analysis.md",
    ]:
        path = job_dir / name
        if path.is_symlink() or path.is_file():
            path.unlink()
        elif path.is_dir():
            shutil.rmtree(path)
    receipt.update(local_cleanup="completed", cleaned_at=datetime.now(UTC).isoformat())
    save_upload_receipt(job_dir, receipt)


async def upload_completed_job(job_dir: Path) -> bool:
    """Upload all declared attempts, including failures, without rerunning them."""
    from harbor.constants import HARBOR_VIEWER_JOBS_URL
    from harbor.upload.uploader import Uploader

    config = json.loads((job_dir / "config.json").read_text())
    result = json.loads((job_dir / "result.json").read_text())
    expected = result["n_total_trials"]
    stats = result["stats"]
    receipt_path = job_dir / "hub-upload.json"
    previous = json.loads(receipt_path.read_text()) if receipt_path.exists() else {}
    confirmed = (
        previous.get("verified_at")
        and previous.get("job_id") == result["id"]
        and previous.get("expected_trials") == expected
    )
    trial_paths = sorted(job_dir.glob("*/result.json"))
    if not is_model_job(config):
        raise ValueError("oracle and nop preflight jobs are not uploaded")
    if (
        not result.get("finished_at")
        or stats.get("n_completed_trials") != expected
        or stats.get("n_cancelled_trials", 0)
        or (not confirmed and len(trial_paths) != expected)
    ):
        raise ValueError("only finished jobs with all declared attempts are uploaded")
    if (
        confirmed
        and previous.get("local_cleanup") == "completed"
        and previous.get("dataset_verified_at")
    ):
        print(f"Already uploaded and cleaned: {previous['url']}")
        return True
    receipt = (
        previous
        if confirmed
        else {
            "status": "uploading",
            "job_id": result["id"],
            "org": "yamaa",
            "visibility": "private",
            "expected_trials": expected,
            "started_at": datetime.now(UTC).isoformat(),
            "trial_ids": [json.loads(p.read_text())["id"] for p in trial_paths],
            "trial_directories": [p.parent.name for p in trial_paths],
        }
    )
    save_upload_receipt(job_dir, receipt)
    try:
        uploader = Uploader()
        if not receipt.get("datasets"):
            from hub import publish_job_datasets

            receipt["datasets"] = await publish_job_datasets(job_dir)
            save_upload_receipt(job_dir, receipt)
        if not confirmed:
            uploaded = await uploader.upload_job(
                job_dir, org="yamaa", visibility="private"
            )
            if uploaded.job_id != result["id"]:
                raise RuntimeError("Harbor Hub returned a different job identity")
            receipt.update(
                job_id=uploaded.job_id,
                url=f"{HARBOR_VIEWER_JOBS_URL}/{uploaded.job_id}",
                uploaded_trials=uploaded.n_trials_uploaded,
                skipped_trials=uploaded.n_trials_skipped,
                failed_trials=uploaded.n_trials_failed,
            )
            if (
                uploaded.n_trials_failed
                or uploaded.n_trials_uploaded + uploaded.n_trials_skipped != expected
            ):
                raise RuntimeError("Harbor Hub did not receive every trial")
        await confirm_uploaded_job(uploader, receipt)
        receipt["status"] = "uploaded"
    except Exception as error:  # noqa: BLE001 -- upload failures must not stop later model jobs
        receipt.update(status="failed", error=str(error))
        print(f"Harbor Hub upload failed for {job_dir}: {error}", file=sys.stderr)
        print(
            f"Retry with: python {Path(__file__)} --upload-only {job_dir}",
            file=sys.stderr,
        )
    receipt["finished_at"] = datetime.now(UTC).isoformat()
    save_upload_receipt(job_dir, receipt)
    if receipt["status"] == "uploaded":
        print(f"Uploaded {expected} trials privately to yamaa: {receipt['url']}")
        try:
            cleanup_uploaded_job(job_dir, receipt)
            print(f"Removed confirmed job artifacts from {job_dir}")
        except OSError as error:
            receipt.update(local_cleanup="failed", cleanup_error=str(error))
            save_upload_receipt(job_dir, receipt)
            print(f"Local cleanup failed for {job_dir}: {error}", file=sys.stderr)
    return receipt["status"] == "uploaded"


def preserve_tasks(config: dict, job_dir: Path) -> dict:
    """Make a new job's tasks independent of mutable build directories."""
    if not config.get("tasks") or config.get("datasets") or config.get("source_jobs"):
        raise ValueError("ranked evaluations require explicit local tasks")
    job_dir.mkdir(parents=True, exist_ok=False)
    config = json.loads(json.dumps(config))
    tasks = {}
    for entry in config["tasks"]:
        source = Path(entry["path"]).resolve()
        task = tomllib.loads((source / "task.toml").read_text())
        name = task["task"]["name"]
        metadata = task.get("metadata", {})
        language, tier = metadata.get("language"), metadata.get("prompt")
        if not entry.get("source") and language and tier:
            suffix = language if tier == "full" else f"{tier}-{language}"
            entry["source"] = f"yamaa/yamaa-sdtm-adam-{suffix}"
        if name in tasks:
            raise ValueError(f"duplicate task: {name}")
        destination = job_dir / "task-snapshots" / source.name
        shutil.copytree(source, destination)
        entry["path"] = str(destination.resolve())
        tasks[name] = {
            "path": entry["path"],
            "source": entry.get("source"),
            "task": task,
        }
    (job_dir / "evaluation.json").write_text(
        json.dumps({"tasks": tasks}, indent=2, sort_keys=True) + "\n"
    )
    return config


async def run(config: dict) -> None:
    from harbor.job import Job
    from harbor.models.job.config import JobConfig
    from harbor.trial.hooks import TrialEvent

    parsed = JobConfig.model_validate(config)
    job_dir = (parsed.jobs_dir / parsed.job_name).resolve()
    saved = preserve_tasks(json.loads(parsed.model_dump_json()), job_dir)
    job = await Job.create(JobConfig.model_validate(saved))
    manifest = json.loads((job_dir / "evaluation.json").read_text())

    async def save_trial_evidence(event):
        # Harbor includes verifier/ in uploaded trial archives, including
        # trials that fail during setup; arbitrary job-root files are excluded.
        entry = manifest["tasks"][event.task_name]
        verifier = job_dir / event.trial_name / "verifier"
        verifier.mkdir(parents=True, exist_ok=True)
        snapshot = verifier / "task.toml"
        if not snapshot.is_file():
            shutil.copyfile(Path(entry["path"]) / "task.toml", snapshot)
        (verifier / "evaluation.json").write_text(
            json.dumps(
                {
                    "task": entry,
                    "expected_tasks": sorted(manifest["tasks"]),
                    "n_attempts": parsed.n_attempts,
                },
                sort_keys=True,
            )
            + "\n"
        )

    job.add_hook(TrialEvent.START, save_trial_evidence)
    # Separate verification empties the mounted verifier directory. Restore
    # the manifest afterward, retaining the verifier's own task snapshot so
    # the collector can detect disagreement with the pre-execution evidence.
    job.add_hook(TrialEvent.END, save_trial_evidence)
    await job.run()
    if is_model_job(saved):
        await upload_completed_job(job_dir)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    action = parser.add_mutually_exclusive_group(required=True)
    action.add_argument("-c", "--config", type=Path)
    action.add_argument(
        "--upload-only", type=Path, help="retry a completed job's upload"
    )
    args = parser.parse_args()
    if args.upload_only:
        if not asyncio.run(upload_completed_job(args.upload_only.resolve())):
            raise SystemExit(1)
    else:
        asyncio.run(run(json.loads(args.config.read_text())))


if __name__ == "__main__":
    main()
