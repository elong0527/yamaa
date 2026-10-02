"""A Hub dataset must describe every task actually evaluated by its job."""

from __future__ import annotations

import asyncio
import copy
import json
from pathlib import Path
from types import SimpleNamespace

import pytest
from test_harbor_evaluation import _load

JOB_ID = "00000000-0000-0000-0000-000000000100"
DATASET = {
    "name": "yamaa/yamaa-sdtm-adam-python",
    "revision": 5,
    "version_id": "dataset-five",
    "task_names": ["yamaa/one", "yamaa/two"],
}


@pytest.fixture
def hub():
    return _load("hub")


@pytest.fixture
def remote(monkeypatch):
    auth = pytest.importorskip("harbor.auth.client")
    db = pytest.importorskip("harbor.db.client")
    client_module = pytest.importorskip("harbor.hub.client")

    def member(name, key):
        return SimpleNamespace(
            task_version=SimpleNamespace(
                content_hash=key,
                package=SimpleNamespace(name=name, org=SimpleNamespace(name="yamaa")),
            )
        )

    state = SimpleNamespace(
        members=[
            member("one", "native-version-one"),
            member("two", "native-version-two"),
        ],
        version={"id": "dataset-five", "revision": 5},
        job={"config": {"tasks": [{"path": "/saved/one"}, {"path": "/saved/two"}]}},
        trials=[
            {
                "id": f"trial-{name}",
                "job_id": JOB_ID,
                "task_name": f"yamaa/{name}",
                "task_content_hash": f"native-version-{name}",
                "config": {"task": {"path": f"/saved/{name}"}},
                "rewards": {"reward": reward},
            }
            for name, reward in (("one", 1), ("two", 0))
        ],
        writes=[],
        reported_datasets=None,
    )

    class Registry:
        async def resolve_dataset_version(self, org, name, ref):
            assert org == "yamaa" and name == "yamaa-sdtm-adam-python" and ref == "5"
            return {}, state.version

        async def get_dataset_version_tasks(self, version_id):
            assert version_id == "dataset-five"
            return state.members

    class Query:
        def __init__(self, table):
            self.table = table
            self.filters = {}
            self.update_value = None
            self.bounds = None
            self.is_single = False

        def select(self, columns):
            return self

        def eq(self, column, value):
            self.filters[column] = value
            return self

        def order(self, column):
            return self

        def range(self, start, end):
            self.bounds = (start, end)
            return self

        def single(self):
            self.is_single = True
            return self

        def update(self, value):
            self.update_value = value
            return self

        async def execute(self):
            if self.table == "job":
                assert self.filters["id"] == JOB_ID
                rows = [state.job]
            else:
                assert self.filters["job_id"] == JOB_ID
                rows = [
                    t
                    for t in state.trials
                    if all(t.get(k) == v for k, v in self.filters.items())
                ]
            if self.update_value is not None:
                state.writes.append((self.table, self.filters, self.update_value))
                for row in rows:
                    row.update(copy.deepcopy(self.update_value))
            elif self.bounds:
                start, end = self.bounds
                rows = rows[start : end + 1]
            result = rows[0] if self.is_single else rows
            return SimpleNamespace(data=copy.deepcopy(result))

    class Client:
        def table(self, table):
            return Query(table)

    async def client():
        return Client()

    class Viewer:
        async def get_job_overview(self, job_ids):
            assert job_ids == [JOB_ID]
            names = state.reported_datasets
            if names is None:
                names = [d["name"] for d in state.job["config"].get("datasets", [])]
            return SimpleNamespace(raw={"datasets": names})

    monkeypatch.setattr(auth, "create_authenticated_client", client)
    monkeypatch.setattr(db, "RegistryDB", Registry)
    monkeypatch.setattr(client_module, "HubClient", Viewer)
    return state


def test_hub_links_exact_dataset_revision_and_preserves_failed_trials(hub, remote):
    before = copy.deepcopy(remote.trials)
    asyncio.run(hub.link_job_datasets(JOB_ID, [DATASET], 2))
    config = remote.job["config"]
    assert config["datasets"] == [{"name": DATASET["name"], "ref": "5"}]
    assert "tasks" not in config
    assert [t["path"] for t in config["local_task_snapshots"]] == [
        "/saved/one",
        "/saved/two",
    ]
    assert config["hub_datasets"][0]["version_id"] == "dataset-five"
    assert [t["rewards"] for t in remote.trials] == [t["rewards"] for t in before]
    assert {t["config"]["task"]["source"] for t in remote.trials} == {DATASET["name"]}
    writes = len(remote.writes)
    asyncio.run(hub.link_job_datasets(JOB_ID, [DATASET], 2))
    assert len(remote.writes) == writes + 1  # Idempotent job metadata, no trial writes.
    assert len(config["local_task_snapshots"]) == 2


@pytest.mark.parametrize(
    "mismatch",
    [
        "revision",
        "missing_member",
        "inaccessible",
        "wrong_version",
        "extra_trial",
        "config",
    ],
)
def test_hub_refuses_mismatched_dataset_before_labeling_trials(hub, remote, mismatch):
    if mismatch == "revision":
        remote.version["id"] = "another-revision"
    elif mismatch == "missing_member":
        remote.members.pop()
    elif mismatch == "inaccessible":
        remote.members[0].task_version = None
    elif mismatch == "wrong_version":
        remote.trials[0]["task_content_hash"] = "different-native-version"
    elif mismatch == "extra_trial":
        remote.trials.append(copy.deepcopy(remote.trials[0]))
    else:
        remote.job["config"]["tasks"][0]["path"] = "/other/task"
    with pytest.raises(ValueError):
        asyncio.run(hub.link_job_datasets(JOB_ID, [DATASET], 2))
    assert not remote.writes


def test_hub_requires_the_dataset_link_in_the_job_page(hub, remote):
    remote.reported_datasets = ["one", "two"]
    with pytest.raises(RuntimeError, match="dataset links"):
        asyncio.run(hub.link_job_datasets(JOB_ID, [DATASET], 2))


def test_hub_refuses_to_assign_one_task_to_multiple_datasets(hub, remote):
    with pytest.raises(ValueError, match="more than one"):
        asyncio.run(hub.link_job_datasets(JOB_ID, [DATASET, DATASET], 2))
    assert not remote.writes


def test_hub_requires_a_declared_dataset(hub, remote):
    with pytest.raises(ValueError, match="published dataset"):
        asyncio.run(hub.link_job_datasets(JOB_ID, [], 2))
    assert not remote.writes


def test_hub_checks_every_attempt_past_the_api_page_limit(hub, remote):
    first = remote.trials[0]
    remote.trials.extend(
        {**copy.deepcopy(first), "id": f"trial-repeat-{n}"} for n in range(1001)
    )
    asyncio.run(hub.link_job_datasets(JOB_ID, [DATASET], 1003))
    assert all(t["config"]["task"]["source"] == DATASET["name"] for t in remote.trials)


def test_publication_uses_preserved_tasks_and_respects_custom_dataset_names(
    hub, tmp_path, monkeypatch
):
    publisher_module = pytest.importorskip("harbor.publisher.publisher")
    manifest_module = pytest.importorskip("harbor.models.dataset.manifest")
    tasks = {}
    for name in ("one", "two"):
        folder = tmp_path / "task-snapshots" / name
        folder.mkdir(parents=True)
        (folder / "task.toml").write_text("preserved task")
        tasks[f"yamaa/{name}"] = {
            "path": str(folder),
            "source": "yamaa/custom-python",
            "task": {
                "metadata": {
                    "prompt": "full",
                    "language": "python",
                    "yamaa_commit": "source-commit",
                }
            },
        }
    (tmp_path / "evaluation.json").write_text(json.dumps({"tasks": tasks}))
    published_tasks, published_datasets = [], []

    class Registry:
        async def resolve_dataset_version(self, org, name, ref):
            assert (org, name, ref) == ("yamaa", "custom-python", "7")
            return {}, {"id": "custom-seven", "revision": 7}

    class Publisher:
        registry_db = Registry()

        async def publish_task(self, folder, visibility):
            assert visibility == "private"
            published_tasks.append(folder)

        async def publish_dataset(self, folder, visibility):
            assert visibility == "private"
            assert "source-commit" in (folder / "README.md").read_text()
            published_datasets.append(folder)
            return SimpleNamespace(revision=7)

    async def command(log, *args):
        if args[0] == "dataset":
            folder = Path(args[-1])
            folder.mkdir()
            (folder / "dataset.toml").write_text("native Harbor manifest")

    monkeypatch.setattr(publisher_module, "Publisher", Publisher)
    monkeypatch.setattr(hub, "harbor_command", command)
    monkeypatch.setattr(
        manifest_module.DatasetManifest,
        "from_toml_file",
        lambda path: SimpleNamespace(tasks=[SimpleNamespace(name=n) for n in tasks]),
    )
    datasets = asyncio.run(hub.publish_job_datasets(tmp_path))
    assert sorted(published_tasks) == sorted(Path(t["path"]) for t in tasks.values())
    assert len(published_datasets) == 1
    assert datasets[0]["name"] == "yamaa/custom-python"
    assert datasets[0]["revision"] == 7 and datasets[0]["version_id"] == "custom-seven"
    assert datasets[0]["task_names"] == sorted(tasks)


@pytest.mark.parametrize("source", ["yamaa/..", "another-org/dataset"])
def test_publication_rejects_unsafe_or_foreign_dataset_names_before_upload(
    hub, tmp_path, monkeypatch, source
):
    publisher_module = pytest.importorskip("harbor.publisher.publisher")
    (tmp_path / "evaluation.json").write_text(
        json.dumps(
            {
                "tasks": {
                    "yamaa/one": {
                        "path": "unused",
                        "source": source,
                        "task": {"metadata": {"prompt": "full", "language": "python"}},
                    }
                }
            }
        )
    )

    def unexpected():
        pytest.fail("invalid dataset names reached the publisher")

    monkeypatch.setattr(publisher_module, "Publisher", unexpected)
    with pytest.raises(ValueError):
        asyncio.run(hub.publish_job_datasets(tmp_path))
