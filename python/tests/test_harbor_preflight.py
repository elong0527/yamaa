"""Keep preflight bounded and publish complete model jobs without losing attempts."""

from __future__ import annotations

import asyncio
import json
import shutil
import sys
from pathlib import Path
from types import SimpleNamespace

import pytest
from test_harbor_evaluation import _load, build

JOB_ID = "00000000-0000-0000-0000-000000000100"
TRIAL_IDS = [
    "00000000-0000-0000-0000-000000000001",
    "00000000-0000-0000-0000-000000000002",
]
DATASETS = [
    {
        "name": "yamaa/yamaa-sdtm-adam-python",
        "revision": 5,
        "version_id": "dataset-version",
        "task_names": ["yamaa/one", "yamaa/two"],
    }
]


@pytest.fixture(autouse=True)
def hub(monkeypatch):
    module = _load("hub")
    monkeypatch.setitem(sys.modules, "hub", module)

    async def publish(job_dir):
        return DATASETS

    async def link(job_id, datasets, expected):
        assert job_id == JOB_ID and datasets == DATASETS and expected == 2

    monkeypatch.setattr(module, "publish_job_datasets", publish)
    monkeypatch.setattr(module, "link_job_datasets", link)

    async def rank(submissions):
        assert submissions == [
            {"row": {"trial_ids": TRIAL_IDS, "metadata": {"job_id": JOB_ID}}}
        ]
        return [{"url": "https://hub.harborframework.com/leaderboards/test"}]

    monkeypatch.setattr(
        module,
        "prepare_job_leaderboards",
        lambda *_: [{"row": {"trial_ids": TRIAL_IDS, "metadata": {"job_id": JOB_ID}}}],
    )
    monkeypatch.setattr(module, "publish_job_leaderboards", rank)
    return module


@pytest.fixture
def smoke(monkeypatch):
    monkeypatch.setitem(sys.modules, "build", build)
    return _load("smoke")


def test_preflight_sample_is_bounded_unique_and_reproducible(smoke):
    candidates = [Path(f"task-{i:03}") for i in range(100)]
    selected = smoke.sample_tasks(candidates, 42)
    assert len(selected) == len(set(selected)) == 10
    assert set(selected) <= set(candidates)
    assert selected == smoke.sample_tasks(list(reversed(candidates)), 42)
    assert selected != smoke.sample_tasks(candidates, 43)
    assert smoke.sample_tasks(candidates[:6], 42) == candidates[:6]


def test_preflight_runs_both_agents_on_the_saved_ten_task_sample(
    tmp_path, monkeypatch, smoke
):
    tasks = []
    names = ["adam-adsl-age-group-python", *[f"task-{i:02}-r" for i in range(14)]]
    for name in names:
        task = tmp_path / "tasks" / name
        (task / "tests").mkdir(parents=True)
        (task / "task.toml").write_text(f'[task]\nname = "yamaa/{name}"\n')
        (task / "tests/contract.json").write_text('{"challenge_required": true}')
        tasks.append(task)
    monkeypatch.setattr(build, "build_selection", lambda *a, **kw: (tasks, []))
    monkeypatch.setattr(build, "git_commit", lambda: "test")
    runner = _load("run")
    jobs = []

    def run_command(command, **kwargs):
        if command[0] == "docker":
            return SimpleNamespace(returncode=0)
        config = json.loads(Path(command[-1]).read_text())
        jobs.append(config)
        agent = config["agents"][0]["name"]
        job_dir = Path(config["jobs_dir"]) / agent
        saved = runner.preserve_tasks(config, job_dir)
        manifest = json.loads((job_dir / "evaluation.json").read_text())["tasks"]
        for entry in saved["tasks"]:
            task = Path(entry["path"])
            name = f"yamaa/{task.name}"
            trial = job_dir / task.name
            verifier = trial / "verifier"
            verifier.mkdir(parents=True)
            shutil.copyfile(task / "task.toml", verifier / "task.toml")
            (verifier / "evaluation.json").write_text(
                json.dumps(
                    {
                        "task": manifest[name],
                        "expected_tasks": sorted(manifest),
                        "n_attempts": 1,
                    }
                )
            )
            (trial / "result.json").write_text(
                json.dumps(
                    {
                        "task_name": name,
                        "config": {"task": entry},
                        "verifier_result": {
                            "rewards": {
                                "reward": float(agent == "oracle"),
                                "challenge_checked": float(agent == "oracle"),
                            }
                        },
                    }
                )
            )
        return SimpleNamespace(returncode=0)

    monkeypatch.setattr(smoke.subprocess, "run", run_command)
    monkeypatch.setattr(
        sys, "argv", ["smoke.py", "--out", str(tmp_path), "--seed", "42"]
    )
    smoke.main()
    sample = json.loads((tmp_path / "sample.json").read_text())
    assert sample["seed"] == 42 and sample["candidate_tasks"] == 15
    assert [j["agents"][0]["name"] for j in jobs] == ["oracle", "nop"]
    assert jobs[0]["tasks"] == jobs[1]["tasks"]
    assert [Path(t["path"]).name for t in jobs[0]["tasks"]] == sample["tasks"]
    assert len(jobs[0]["tasks"]) == 10


@pytest.fixture
def completed_job(tmp_path):
    (tmp_path / "config.json").write_text(
        json.dumps({"agents": [{"name": "opencode", "model_name": "provider/model"}]})
    )
    (tmp_path / "result.json").write_text(
        json.dumps(
            {
                "id": JOB_ID,
                "finished_at": "2026-10-02T12:00:00Z",
                "n_total_trials": 2,
                "stats": {"n_completed_trials": 2, "n_errored_trials": 1},
            }
        )
    )
    for trial_id, (name, exception) in zip(
        TRIAL_IDS, [("passed", None), ("failed", {"type": "APIError"})], strict=True
    ):
        trial = tmp_path / name
        trial.mkdir()
        (trial / "result.json").write_text(
            json.dumps({"id": trial_id, "exception_info": exception})
        )
        (trial / "artifact.txt").write_text("model output")
    for name in ("task-snapshots", ".harbor-upload"):
        (tmp_path / name).mkdir()
        (tmp_path / name / "data.txt").write_text("local cache")
    for name in ("job.log", "lock.json", "evaluation.json"):
        (tmp_path / name).write_text("{}")
    (tmp_path / "notes.txt").write_text("unrelated user notes")
    return tmp_path


@pytest.fixture
def hub_db():
    class FakeDB:
        def __init__(self):
            self.job = {
                "archive_path": f"jobs/{JOB_ID}/job.tar.gz",
                "finished_at": "2026-10-02T12:00:00Z",
                "n_planned_trials": 2,
            }
            self.trials = [
                {"id": trial_id, "archive_path": f"trials/{trial_id}/trial.tar.gz"}
                for trial_id in TRIAL_IDS
            ]

        async def get_job(self, job_id):
            assert str(job_id) == JOB_ID
            return self.job

        async def list_trials_for_job(self, job_id):
            assert str(job_id) == JOB_ID
            return self.trials

    return FakeDB()


@pytest.mark.parametrize("name", ["", "..", "../outside"])
def test_cleanup_rejects_paths_outside_the_job(completed_job, name):
    runner = _load("run")
    receipt = {
        "status": "uploaded",
        "verified_at": "verified",
        "dataset_verified_at": "verified",
        "datasets": DATASETS,
        "leaderboard_verified_at": "verified",
        "leaderboards": [{"id": "board"}],
        "trial_directories": [name],
    }
    with pytest.raises(ValueError):
        runner.cleanup_uploaded_job(completed_job, receipt)
    assert (completed_job / "passed/artifact.txt").is_file()


def test_cleanup_requires_remote_confirmation(completed_job):
    runner = _load("run")
    with pytest.raises(ValueError, match="confirmed Harbor Hub"):
        runner.cleanup_uploaded_job(completed_job, {"status": "uploaded"})
    assert (completed_job / "passed/artifact.txt").is_file()


def test_cleanup_requires_verified_dataset_links(completed_job):
    runner = _load("run")
    with pytest.raises(ValueError, match="confirmed Harbor Hub"):
        runner.cleanup_uploaded_job(
            completed_job,
            {"status": "uploaded", "verified_at": "verified", "datasets": DATASETS},
        )
    assert (completed_job / "passed/artifact.txt").is_file()


def test_cleanup_requires_verified_leaderboard_scores_and_trial_links(completed_job):
    runner = _load("run")
    with pytest.raises(ValueError, match="confirmed Harbor Hub"):
        runner.cleanup_uploaded_job(
            completed_job,
            {
                "status": "uploaded",
                "verified_at": "verified",
                "dataset_verified_at": "verified",
                "datasets": DATASETS,
            },
        )
    assert (completed_job / "passed/artifact.txt").is_file()


def test_failed_leaderboard_publication_keeps_evidence_and_retries_without_reupload(
    completed_job, monkeypatch, hub_db, hub
):
    uploader = pytest.importorskip("harbor.upload.uploader")
    runner = _load("run")
    uploads = []

    class FakeUploader:
        db = hub_db

        async def upload_job(self, job_dir, **kwargs):
            uploads.append(job_dir)
            return SimpleNamespace(
                job_id=JOB_ID,
                n_trials_uploaded=2,
                n_trials_skipped=0,
                n_trials_failed=0,
            )

    rank = hub.publish_job_leaderboards

    async def unavailable(submissions):
        raise RuntimeError("leaderboard unavailable")

    monkeypatch.setattr(uploader, "Uploader", FakeUploader)
    monkeypatch.setattr(hub, "publish_job_leaderboards", unavailable)
    assert not asyncio.run(runner.upload_completed_job(completed_job))
    receipt = json.loads((completed_job / "hub-upload.json").read_text())
    assert receipt["verified_at"] and receipt["dataset_verified_at"]
    assert receipt["leaderboard_submissions"] and not receipt.get(
        "leaderboard_verified_at"
    )
    assert (completed_job / "passed/artifact.txt").exists()
    monkeypatch.setattr(hub, "publish_job_leaderboards", rank)
    monkeypatch.setattr(
        hub,
        "prepare_job_leaderboards",
        lambda *_: pytest.fail("retry must reuse preserved scores"),
    )
    assert asyncio.run(runner.upload_completed_job(completed_job))
    assert len(uploads) == 1
    receipt = json.loads((completed_job / "hub-upload.json").read_text())
    assert receipt["leaderboard_verified_at"] and "error" not in receipt
    assert not (completed_job / "passed").exists()


@pytest.mark.parametrize("mismatch", ["job", "missing_attempt", "duplicate_attempt"])
def test_cleanup_refuses_a_cached_submission_for_another_job_or_attempt_set(
    completed_job, monkeypatch, hub_db, hub, mismatch
):
    uploader = pytest.importorskip("harbor.upload.uploader")
    runner = _load("run")

    class FakeUploader:
        db = hub_db

        async def upload_job(self, *args, **kwargs):
            return SimpleNamespace(
                job_id=JOB_ID,
                n_trials_uploaded=2,
                n_trials_skipped=0,
                n_trials_failed=0,
            )

    row = {"metadata": {"job_id": JOB_ID}, "trial_ids": list(TRIAL_IDS)}
    if mismatch == "job":
        row["metadata"]["job_id"] = "another-job"
    elif mismatch == "missing_attempt":
        row["trial_ids"].pop()
    else:
        row["trial_ids"][1] = row["trial_ids"][0]
    monkeypatch.setattr(uploader, "Uploader", FakeUploader)
    monkeypatch.setattr(hub, "prepare_job_leaderboards", lambda *_: [{"row": row}])
    assert not asyncio.run(runner.upload_completed_job(completed_job))
    receipt = json.loads((completed_job / "hub-upload.json").read_text())
    assert "every uploaded attempt" in receipt["error"]
    assert not receipt.get("leaderboard_verified_at")
    assert (completed_job / "passed/artifact.txt").exists()


def test_upload_keeps_failed_attempts_and_uses_private_yamaa_org(
    completed_job, monkeypatch, hub_db
):
    uploader = pytest.importorskip("harbor.upload.uploader")
    runner = _load("run")
    calls = []

    class FakeUploader:
        db = hub_db

        async def upload_job(self, job_dir, **kwargs):
            assert (job_dir / "failed/result.json").is_file()
            calls.append((job_dir, kwargs))
            return SimpleNamespace(
                job_id=JOB_ID,
                n_trials_uploaded=2,
                n_trials_skipped=0,
                n_trials_failed=0,
            )

    monkeypatch.setattr(uploader, "Uploader", FakeUploader)
    assert asyncio.run(runner.upload_completed_job(completed_job))
    assert calls == [(completed_job, {"org": "yamaa", "visibility": "private"})]
    receipt = json.loads((completed_job / "hub-upload.json").read_text())
    assert receipt["status"] == "uploaded" and receipt["uploaded_trials"] == 2
    assert receipt["verified_at"] and receipt["local_cleanup"] == "completed"
    assert receipt["datasets"] == DATASETS and receipt["dataset_verified_at"]
    assert receipt["leaderboards"] and receipt["leaderboard_verified_at"]
    assert not (completed_job / "passed").exists()
    assert not (completed_job / "failed").exists()
    assert not (completed_job / "task-snapshots").exists()
    assert not (completed_job / ".harbor-upload").exists()
    assert not (completed_job / "job.log").exists()
    assert (completed_job / "result.json").is_file()
    assert (completed_job / "config.json").is_file()
    assert (completed_job / "notes.txt").read_text() == "unrelated user notes"
    # Repeating a successful upload after cleanup does not upload incomplete data.
    assert asyncio.run(runner.upload_completed_job(completed_job))
    assert len(calls) == 1


@pytest.mark.parametrize(
    "missing",
    [
        "job",
        "job_archive",
        "job_finished",
        "planned_count",
        "trial",
        "trial_archive",
        "trial_id",
    ],
)
def test_cleanup_waits_for_remote_job_and_every_trial_archive(
    completed_job, monkeypatch, hub_db, missing
):
    uploader = pytest.importorskip("harbor.upload.uploader")
    runner = _load("run")
    if missing == "job":
        hub_db.job = None
    elif missing.startswith("job_"):
        key = "archive_path" if missing == "job_archive" else "finished_at"
        hub_db.job[key] = None
    elif missing == "planned_count":
        hub_db.job["n_planned_trials"] = 3
    elif missing == "trial":
        hub_db.trials.pop()
    elif missing == "trial_archive":
        hub_db.trials[0]["archive_path"] = None
    else:
        hub_db.trials[0]["id"] = "different-trial"

    class FakeUploader:
        db = hub_db

        async def upload_job(self, job_dir, **kwargs):
            return SimpleNamespace(
                job_id=JOB_ID,
                n_trials_uploaded=2,
                n_trials_skipped=0,
                n_trials_failed=0,
            )

    monkeypatch.setattr(uploader, "Uploader", FakeUploader)
    assert not asyncio.run(runner.upload_completed_job(completed_job))
    for name in ("passed", "failed", "task-snapshots", ".harbor-upload"):
        assert (completed_job / name).is_dir()
    receipt = json.loads((completed_job / "hub-upload.json").read_text())
    assert receipt["status"] == "failed" and not receipt.get("verified_at")


def test_cleanup_can_retry_without_uploading_or_rerunning_the_job(
    completed_job, monkeypatch, hub_db
):
    uploader = pytest.importorskip("harbor.upload.uploader")
    runner = _load("run")
    calls = []

    class FakeUploader:
        db = hub_db

        async def upload_job(self, job_dir, **kwargs):
            calls.append(job_dir)
            return SimpleNamespace(
                job_id=JOB_ID,
                n_trials_uploaded=2,
                n_trials_skipped=0,
                n_trials_failed=0,
            )

    remove_tree = runner.shutil.rmtree

    def refuse_cleanup(path):
        if path.name == "passed":
            raise OSError("directory is busy")
        remove_tree(path)

    monkeypatch.setattr(uploader, "Uploader", FakeUploader)
    monkeypatch.setattr(runner.shutil, "rmtree", refuse_cleanup)
    assert asyncio.run(runner.upload_completed_job(completed_job))
    receipt = json.loads((completed_job / "hub-upload.json").read_text())
    assert receipt["status"] == "uploaded" and receipt["local_cleanup"] == "failed"
    assert (completed_job / "passed/artifact.txt").is_file()
    assert not (completed_job / "failed").exists()
    get_job = hub_db.get_job

    async def unavailable(job_id):
        raise RuntimeError("Hub temporarily unavailable")

    monkeypatch.setattr(hub_db, "get_job", unavailable)
    assert not asyncio.run(runner.upload_completed_job(completed_job))
    assert (completed_job / "passed/artifact.txt").is_file()
    monkeypatch.setattr(hub_db, "get_job", get_job)
    monkeypatch.setattr(runner.shutil, "rmtree", remove_tree)
    assert asyncio.run(runner.upload_completed_job(completed_job))
    assert len(calls) == 1
    assert not (completed_job / "passed").exists()
    assert (
        json.loads((completed_job / "hub-upload.json").read_text())["local_cleanup"]
        == "completed"
    )


@pytest.mark.parametrize("agent", ["opencode", "oracle", "nop"])
def test_runner_uploads_only_model_jobs_after_execution(tmp_path, monkeypatch, agent):
    job_module = pytest.importorskip("harbor.job")
    runner = _load("run")
    task = build.build_task(
        build.ROOT / "benchmarks/adam-adsl-age-group",
        tmp_path / "tasks",
        build.IMAGE,
        "test",
        "python",
    )
    calls = []

    class FakeJob:
        def add_hook(self, *args):
            pass

        async def run(self):
            calls.append("run")

    async def create(config):
        return FakeJob()

    async def upload(job_dir):
        assert calls == ["run"]
        calls.append("upload")
        return True

    monkeypatch.setattr(job_module.Job, "create", create)
    monkeypatch.setattr(runner, "upload_completed_job", upload)
    asyncio.run(
        runner.run(
            {
                "jobs_dir": str(tmp_path / "jobs"),
                "job_name": "test",
                "agents": [{"name": agent}],
                "tasks": [{"path": str(task)}],
            }
        )
    )
    assert calls == (["run", "upload"] if agent == "opencode" else ["run"])


@pytest.mark.parametrize(
    "case", ["unfinished", "cancelled", "missing_trial", "oracle", "nop"]
)
def test_upload_rejects_incomplete_or_preflight_jobs(completed_job, monkeypatch, case):
    uploader = pytest.importorskip("harbor.upload.uploader")
    runner = _load("run")
    config = json.loads((completed_job / "config.json").read_text())
    result = json.loads((completed_job / "result.json").read_text())
    if case == "unfinished":
        result["finished_at"] = None
    elif case == "cancelled":
        result["stats"]["n_cancelled_trials"] = 1
    elif case == "missing_trial":
        shutil.rmtree(completed_job / "failed")
    else:
        config["agents"][0]["name"] = case
    (completed_job / "config.json").write_text(json.dumps(config))
    (completed_job / "result.json").write_text(json.dumps(result))

    def unexpected_upload():
        pytest.fail("an incomplete or preflight job reached Harbor Hub")

    monkeypatch.setattr(uploader, "Uploader", unexpected_upload)
    with pytest.raises(ValueError):
        asyncio.run(runner.upload_completed_job(completed_job))


@pytest.mark.parametrize("failure", ["partial", "network"])
def test_upload_failure_preserves_results_and_records_retry(
    completed_job, monkeypatch, failure
):
    uploader = pytest.importorskip("harbor.upload.uploader")
    runner = _load("run")
    before = (completed_job / "result.json").read_bytes()

    class FakeUploader:
        async def upload_job(self, job_dir, **kwargs):
            if failure == "network":
                raise RuntimeError("network unavailable")
            return SimpleNamespace(
                job_id=JOB_ID,
                n_trials_uploaded=1,
                n_trials_skipped=0,
                n_trials_failed=1,
            )

    monkeypatch.setattr(uploader, "Uploader", FakeUploader)
    assert not asyncio.run(runner.upload_completed_job(completed_job))
    assert (completed_job / "result.json").read_bytes() == before
    receipt = json.loads((completed_job / "hub-upload.json").read_text())
    assert receipt["status"] == "failed" and receipt["error"]


@pytest.mark.parametrize("failure", ["publish", "link"])
def test_dataset_failures_preserve_local_results(
    completed_job, monkeypatch, hub, hub_db, failure
):
    uploader = pytest.importorskip("harbor.upload.uploader")
    runner = _load("run")

    class FakeUploader:
        db = hub_db

        async def upload_job(self, job_dir, **kwargs):
            return SimpleNamespace(
                job_id=JOB_ID,
                n_trials_uploaded=2,
                n_trials_skipped=0,
                n_trials_failed=0,
            )

    async def unavailable(*args):
        raise RuntimeError("dataset association unavailable")

    monkeypatch.setattr(uploader, "Uploader", FakeUploader)
    method = "publish_job_datasets" if failure == "publish" else "link_job_datasets"
    monkeypatch.setattr(hub, method, unavailable)
    assert not asyncio.run(runner.upload_completed_job(completed_job))
    assert (completed_job / "passed/artifact.txt").is_file()
    receipt = json.loads((completed_job / "hub-upload.json").read_text())
    assert receipt["status"] == "failed" and not receipt.get("dataset_verified_at")
