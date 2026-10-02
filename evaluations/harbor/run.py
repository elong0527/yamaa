"""Run a local evaluation job with task evidence saved before execution.

    python evaluations/harbor/run.py -c <generated-job.json>

The job owns a copy of each task, including its metadata. Later builds cannot
change its inputs or provenance, even when setup fails before verification.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import shutil
from pathlib import Path

import tomllib


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
        if name in tasks:
            raise ValueError(f"duplicate task: {name}")
        destination = job_dir / "task-snapshots" / source.name
        shutil.copytree(source, destination)
        entry["path"] = str(destination.resolve())
        tasks[name] = {"path": entry["path"], "task": task}
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


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("-c", "--config", required=True, type=Path)
    args = parser.parse_args()
    asyncio.run(run(json.loads(args.config.read_text())))


if __name__ == "__main__":
    main()
