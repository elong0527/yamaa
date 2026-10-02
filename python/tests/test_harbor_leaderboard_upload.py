"""A published row must use task pass rate and retain every attempt as evidence."""

from __future__ import annotations

import asyncio
import copy
import sys
from types import SimpleNamespace

import pytest
from test_harbor_evaluation import _board, _fake_job, _load, leaderboard

VERSION = "00000000-0000-0000-0000-000000000123"
BOARD = "00000000-0000-0000-0000-000000000456"
ROW = "00000000-0000-0000-0000-000000000789"


@pytest.fixture
def hub(monkeypatch):
    monkeypatch.setitem(sys.modules, "leaderboard", leaderboard)
    return _load("hub")


@pytest.fixture
def evidence(tmp_path, monkeypatch):
    tasks = ["b-one", "b-two"]
    board = _board(tasks)
    monkeypatch.setattr(leaderboard, "load_leaderboards", lambda: [board])
    job = _fake_job(tmp_path, {"b-one": [1.0], "b-two": [None]})
    dataset = {
        "name": "yamaa/custom-python",
        "version_id": VERSION,
        "task_names": [f"yamaa/{t}" for t in tasks],
    }
    return job, dataset


@pytest.fixture
def submission(hub, evidence):
    job, dataset = evidence
    return hub.prepare_job_leaderboards(job, [dataset])


def test_prepared_row_pins_release_version_and_counts_errors_as_failed_attempts(
    submission,
):
    item = submission[0]
    row, definition = item["row"], item["definition"]
    assert row["metrics"]["reward"] == 0.5
    assert row["metrics"]["task_pass_rate_display"] == "50.0%"
    assert row["metrics"]["passed_trials_display"] == "1/2"
    assert row["metrics"]["n_errors"] == 1
    assert len(row["trial_ids"]) == 2
    assert row["metadata"]["job_url"].endswith(row["metadata"]["job_id"])
    assert definition["package"] == "yamaa/custom-python"
    assert definition["dataset_version_ids"] == [VERSION]
    assert definition["name"].endswith(row["metadata"]["yamaa_commit"])
    assert definition["rank_by"][0]["accessor"] == "metrics.reward"


def test_preparation_requires_complete_task_set(hub, evidence):
    job, dataset = evidence
    dataset["task_names"].pop()
    with pytest.raises(ValueError, match="complete leaderboard"):
        hub.prepare_job_leaderboards(job, [dataset])


def test_preparation_requires_the_original_grading_evidence(hub, evidence):
    job, dataset = evidence
    for path in job.glob("*/verifier/grade.json"):
        path.unlink()
    with pytest.raises(ValueError, match="grading or trajectory evidence"):
        hub.prepare_job_leaderboards(job, [dataset])


@pytest.fixture
def api(monkeypatch):
    native = pytest.importorskip("harbor.hub.leaderboards")
    state = SimpleNamespace(
        board=None, rows=[], links={}, writes=[], missing_link=False
    )

    class Client:
        async def list_leaderboards(self, package):
            return [state.board] if state.board else []

        async def create(self, definition):
            state.writes.append("create_board")
            state.board = SimpleNamespace(
                id=BOARD, updated_at="2026-10-02T00:00:00Z", **copy.deepcopy(definition)
            )
            return state.board

        async def get(self, leaderboard_id):
            assert leaderboard_id == BOARD
            return state.board

        async def list_rows(self, leaderboard_id, page=1, page_size=1000):
            assert leaderboard_id == BOARD
            start = (page - 1) * page_size
            return state.board, SimpleNamespace(
                items=state.rows[start : start + page_size],
                total_pages=max(1, (len(state.rows) + page_size - 1) // page_size),
            )

        async def create_rows(self, body):
            assert set(body) == {"leaderboard_id", "rows"}
            assert body["leaderboard_id"] == BOARD
            state.writes.append("create_row")
            values = copy.deepcopy(body["rows"][0])
            trial_ids = values.pop("trial_ids")
            state.rows.append(
                SimpleNamespace(id=ROW, updated_at="2026-10-02T00:00:00Z", **values)
            )
            state.links[ROW] = trial_ids[:-1] if state.missing_link else trial_ids
            return {}

        async def get_row(self, row_id):
            return next(r for r in state.rows if r.id == row_id)

        async def list_row_trials(self, row_id, page=1, page_size=1000):
            ids = state.links[row_id]
            start = (page - 1) * page_size
            return SimpleNamespace(
                items=[
                    SimpleNamespace(trial_id=i) for i in ids[start : start + page_size]
                ],
                total_pages=max(1, (len(ids) + page_size - 1) // page_size),
            )

        async def update_definition(self, body):
            state.writes.append("update_board")
            assert body.pop("leaderboard_id") == BOARD
            assert body.pop("expected_updated_at") == state.board.updated_at
            for key, value in body.items():
                setattr(state.board, key, copy.deepcopy(value))
            return {}

        async def update_rows(self, body):
            state.writes.append("update_row")
            update = body["rows"][0]
            row = await self.get_row(update["id"])
            assert update["expected_updated_at"] == row.updated_at
            for key in ("metadata", "metrics", "status"):
                setattr(row, key, copy.deepcopy(update[key]))
            return {}

    monkeypatch.setattr(native, "LeaderboardClient", Client)
    return state


def test_publication_is_idempotent_and_verifies_every_trial(hub, submission, api):
    first = asyncio.run(hub.publish_job_leaderboards(submission))
    assert first[0]["primary_metric"] == "reward"
    assert first[0]["task_pass_rate"] == 0.5
    assert first[0]["passed_trials"] == 1 and first[0]["total_trials"] == 2
    assert api.writes == ["create_board", "create_row"]
    assert asyncio.run(hub.publish_job_leaderboards(submission)) == first
    assert api.writes == ["create_board", "create_row"]


def test_publication_refuses_partial_trial_links(hub, submission, api):
    api.missing_link = True
    with pytest.raises(RuntimeError, match="every leaderboard trial link"):
        asyncio.run(hub.publish_job_leaderboards(submission))


def test_publication_refuses_a_different_primary_metric(hub, submission, api):
    submission[0]["definition"]["rank_by"][0]["accessor"] = "metrics.cell_accuracy"
    with pytest.raises(ValueError, match="primary leaderboard score"):
        asyncio.run(hub.publish_job_leaderboards(submission))
    assert not api.writes


def test_retry_reuses_existing_row_and_updates_definition_and_dataset_versions(
    hub, submission, api
):
    asyncio.run(hub.publish_job_leaderboards(submission))
    api.board.rank_by.reverse()
    api.board.dataset_version_ids = ["00000000-0000-0000-0000-000000000124"]
    api.rows[0].status = "hide"
    asyncio.run(hub.publish_job_leaderboards(submission))
    assert len(api.rows) == 1
    assert api.board.rank_by[0]["accessor"] == "metrics.reward"
    assert set(api.board.dataset_version_ids) == {
        VERSION,
        "00000000-0000-0000-0000-000000000124",
    }
    assert api.rows[0].status == "display"
    assert api.writes[-2:] == ["update_board", "update_row"]


def test_publication_refuses_incompatible_release_board(hub, submission, api):
    asyncio.run(hub.publish_job_leaderboards(submission))
    api.board.metadata_schema["properties"]["yamaa_commit"]["const"] = "other-release"
    with pytest.raises(ValueError, match="different task release"):
        asyncio.run(hub.publish_job_leaderboards(submission))
    assert len(api.writes) == 2


def test_retry_finds_the_job_after_the_first_page(hub, submission, api):
    asyncio.run(hub.publish_job_leaderboards(submission))
    api.rows[:0] = [
        SimpleNamespace(id=f"other-{n}", metadata={"job_id": f"other-{n}"})
        for n in range(1000)
    ]
    asyncio.run(hub.publish_job_leaderboards(submission))
    assert api.writes == ["create_board", "create_row"]


def test_retry_refuses_duplicate_rows_for_the_same_job(hub, submission, api):
    asyncio.run(hub.publish_job_leaderboards(submission))
    api.rows.append(copy.deepcopy(api.rows[0]))
    with pytest.raises(RuntimeError, match="duplicate leaderboard rows"):
        asyncio.run(hub.publish_job_leaderboards(submission))
