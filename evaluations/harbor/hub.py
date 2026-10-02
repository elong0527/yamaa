"""Publish immutable task snapshots and verify their jobs' dataset associations.

Harbor owns its package manifests and version resolution. yamaa records the
returned dataset revisions and IDs and never computes its own content identity.
"""

from __future__ import annotations

import asyncio
import json
import sys
from collections import defaultdict
from pathlib import Path


def prepare_job_leaderboards(job_dir: Path, datasets: list[dict]) -> list[dict]:
    """Calculate ranked rows from complete local evidence before any cleanup."""
    from leaderboard import collect, definition_for, load_leaderboards, rows_for

    try:
        run = collect(job_dir)
    except SystemExit as error:
        raise ValueError(str(error)) from error
    submissions = []
    for dataset in datasets:
        task_names = {name.split("/", 1)[1] for name in dataset["task_names"]}
        boards = [b for b in load_leaderboards() if set(b["tasks"]) == task_names]
        if len(boards) != 1:
            raise ValueError(
                "a ranked job must cover one complete leaderboard task set"
            )
        board = boards[0]
        run["leaderboards"] = [board["harbor"]["name"]]
        try:
            rows = rows_for(board, [run])
        except SystemExit as error:
            raise ValueError(str(error)) from error
        definition = definition_for(board, rows, dataset["name"])
        definition["dataset_version_ids"] = [dataset["version_id"]]
        submissions.append(
            {
                "definition": definition,
                "row": {
                    k: rows[0][k]
                    for k in ("metadata", "metrics", "status", "trial_ids")
                },
            }
        )
    if not submissions:
        raise ValueError("a ranked job must have a dataset")
    return submissions


async def _leaderboard_rows(client, board_id: str):
    rows, page = [], 1
    while True:
        board, response = await client.list_rows(
            leaderboard_id=board_id, page=page, page_size=1000
        )
        rows.extend(response.items)
        if page >= response.total_pages:
            return board, rows
        page += 1


async def publish_job_leaderboards(submissions: list[dict]) -> list[dict]:
    """Publish once per job, then confirm scores, ranking, versions and trial links."""
    from harbor.hub.leaderboards import (
        LeaderboardClient,
        LeaderboardCreateConfig,
        LeaderboardDefinitionUpdateConfig,
        LeaderboardRowCreate,
    )

    client = LeaderboardClient()
    published = []
    for submission in submissions:
        definition = LeaderboardCreateConfig.model_validate(
            submission["definition"]
        ).to_request()
        desired = LeaderboardRowCreate.model_validate(submission["row"]).to_request()
        if definition["rank_by"][0]["accessor"] != "metrics.reward":
            raise ValueError("task pass rate must be the primary leaderboard score")
        package, name = definition["package"], definition["name"]
        existing = [
            b for b in await client.list_leaderboards(package=package) if b.name == name
        ]
        if len(existing) > 1:
            raise RuntimeError("Harbor returned duplicate leaderboard identities")
        if existing:
            board = await client.get(leaderboard_id=existing[0].id)
            for key in ("yamaa_commit", "image_reference"):
                actual = (
                    board.metadata_schema.get("properties", {})
                    .get(key, {})
                    .get("const")
                )
                expected = definition["metadata_schema"]["properties"][key]["const"]
                if actual != expected:
                    raise ValueError(
                        "existing leaderboard covers a different task release or runtime"
                    )
            changes = {
                k: v
                for k, v in definition.items()
                if k not in ("package", "name", "dataset_version_ids")
                and getattr(board, k) != v
            }
            versions = sorted(
                set(board.dataset_version_ids or [])
                | set(definition["dataset_version_ids"])
            )
            if set(versions) != set(board.dataset_version_ids or []):
                changes["dataset_version_ids"] = versions
            if changes:
                update = LeaderboardDefinitionUpdateConfig.model_validate(
                    changes
                ).to_request()
                await client.update_definition(
                    {
                        "leaderboard_id": board.id,
                        "expected_updated_at": board.updated_at,
                        **update,
                    }
                )
        else:
            board = await client.create(definition)

        board, rows = await _leaderboard_rows(client, board.id)
        matches = [
            r for r in rows if r.metadata.get("job_id") == desired["metadata"]["job_id"]
        ]
        if len(matches) > 1:
            raise RuntimeError("the job already has duplicate leaderboard rows")
        if not matches:
            await client.create_rows({"leaderboard_id": board.id, "rows": [desired]})
            board, rows = await _leaderboard_rows(client, board.id)
            matches = [
                r
                for r in rows
                if r.metadata.get("job_id") == desired["metadata"]["job_id"]
            ]
        if len(matches) != 1:
            raise RuntimeError("Harbor has not confirmed the job's leaderboard row")
        row = matches[0]
        if any(
            getattr(row, key) != desired[key]
            for key in ("metadata", "metrics", "status")
        ):
            await client.update_rows(
                {
                    "rows": [
                        {
                            "id": row.id,
                            "expected_updated_at": row.updated_at,
                            **{
                                k: desired[k] for k in ("metadata", "metrics", "status")
                            },
                        }
                    ]
                }
            )
            row = await client.get_row(row.id)
        linked, page = set(), 1
        while True:
            response = await client.list_row_trials(row.id, page=page, page_size=1000)
            linked.update(t.trial_id for t in response.items)
            if page >= response.total_pages:
                break
            page += 1
        if linked != set(desired["trial_ids"]):
            raise RuntimeError("Harbor has not confirmed every leaderboard trial link")
        board = await client.get(leaderboard_id=board.id)
        row = await client.get_row(row.id)
        if (
            any(
                getattr(row, key) != desired[key]
                for key in ("metadata", "metrics", "status")
            )
            or any(
                getattr(board, key) != definition[key]
                for key in (
                    "rank_by",
                    "columns",
                    "metadata_schema",
                    "metrics_schema",
                    "visibility",
                )
            )
            or not set(definition["dataset_version_ids"])
            <= set(board.dataset_version_ids or [])
        ):
            raise RuntimeError(
                "Harbor has not confirmed the leaderboard scores and definition"
            )
        published.append(
            {
                "id": board.id,
                "name": name,
                "package": package,
                "row_id": row.id,
                "url": f"https://hub.harborframework.com/datasets/{package}?tab=leaderboard&leaderboard={name}",
                "primary_metric": "reward",
                "task_pass_rate": row.metrics["reward"],
                "passed_trials": row.metrics["n_passed_trials"],
                "total_trials": row.metrics["n_trials"],
            }
        )
    if not published:
        raise ValueError("a ranked job must have a leaderboard submission")
    return published


async def harbor_command(log: Path, *args: str) -> None:
    """Use the job's installed Harbor CLI to write its native dataset manifest."""
    with log.open("a") as output:
        process = await asyncio.create_subprocess_exec(
            sys.executable,
            "-c",
            "from harbor.cli.main import app; app()",
            *args,
            stdout=output,
            stderr=output,
        )
        if await process.wait():
            raise RuntimeError(f"Harbor {args[0]} failed; see {log}")


async def publish_job_datasets(job_dir: Path) -> list[dict]:
    """Publish exactly the preserved tasks, grouped by language and prompt tier."""
    from harbor.models.dataset.manifest import DatasetInfo, DatasetManifest
    from harbor.publisher.publisher import Publisher

    manifest = json.loads((job_dir / "evaluation.json").read_text())
    groups = defaultdict(list)
    for name, entry in manifest["tasks"].items():
        metadata = entry["task"]["metadata"]
        tier, language = metadata["prompt"], metadata["language"]
        if tier not in ("full", "conventions", "brief") or language not in (
            "python",
            "r",
        ):
            raise ValueError("unknown language or prompt tier in job evidence")
        suffix = language if tier == "full" else f"{tier}-{language}"
        source = entry.get("source") or f"yamaa/yamaa-sdtm-adam-{suffix}"
        if not source.startswith("yamaa/") or source.count("/") != 1:
            raise ValueError("model datasets must belong to the yamaa organization")
        DatasetInfo(name=source)
        groups[source].append((name, entry))
    if not groups:
        raise ValueError("no preserved tasks to publish")
    for entries in groups.values():
        tracks = {
            (e["task"]["metadata"]["prompt"], e["task"]["metadata"]["language"])
            for _, e in entries
        }
        if len(tracks) != 1:
            raise ValueError("a dataset must have one language and prompt tier")
    publisher = Publisher()
    semaphore = asyncio.Semaphore(12)

    async def publish_task(entry):
        async with semaphore:
            await publisher.publish_task(Path(entry["path"]), visibility="private")

    await asyncio.gather(
        *(publish_task(entry) for entries in groups.values() for _, entry in entries)
    )
    datasets = []
    root = job_dir / "hub-datasets"
    root.mkdir(exist_ok=True)
    for source, entries in sorted(groups.items()):
        folder = root / source.split("/")[1]
        log = root / "publish.log"
        if not (folder / "dataset.toml").exists():
            await harbor_command(log, "dataset", "init", source, "-o", str(folder))
        task_names = sorted(name for name, _ in entries)
        commits = sorted({e["task"]["metadata"]["yamaa_commit"] for _, e in entries})
        (folder / "README.md").write_text(
            f"# {source}\n\n"
            f"{len(entries)} yamaa SDTM and ADaM benchmark tasks.\n\n"
            f"Tasks from: {', '.join(f'`{commit}`' for commit in commits)}.\n\n"
            + "\n".join(f"- `{name}`" for name in task_names)
            + "\n"
        )
        await harbor_command(
            log, "add", *(e["path"] for _, e in entries), "--to", str(folder)
        )
        native = DatasetManifest.from_toml_file(folder / "dataset.toml")
        if sorted(t.name for t in native.tasks) != task_names:
            raise ValueError("dataset manifest does not match the preserved task set")
        published = await publisher.publish_dataset(folder, visibility="private")
        org, name = source.split("/")
        _, version = await publisher.registry_db.resolve_dataset_version(
            org, name, str(published.revision)
        )
        datasets.append(
            {
                "name": source,
                "revision": version["revision"],
                "version_id": version["id"],
                "task_names": task_names,
                "url": f"https://hub.harborframework.com/datasets/{source}",
            }
        )
        print(
            f"Published {source} revision {version['revision']}: {len(entries)} tasks"
        )
    return datasets


async def link_job_datasets(job_id: str, datasets: list[dict], expected: int) -> None:
    """Check native registry membership before setting the Hub's source labels."""
    from harbor.auth.client import create_authenticated_client
    from harbor.db.client import RegistryDB
    from harbor.hub.client import HubClient

    if not datasets:
        raise ValueError("a model job must have a published dataset")
    registry = RegistryDB()
    versions, sources = {}, {}
    for dataset in datasets:
        org, name = dataset["name"].split("/")
        _, version = await registry.resolve_dataset_version(
            org, name, str(dataset["revision"])
        )
        if version["id"] != dataset["version_id"]:
            raise ValueError("Harbor resolved a different dataset revision")
        members = await registry.get_dataset_version_tasks(dataset["version_id"])
        own = {}
        for member in members:
            task = member.task_version
            if task is None:
                raise ValueError("a dataset task version is missing or inaccessible")
            task_name = f"{task.package.org.name}/{task.package.name}"
            # Compare Harbor's native version keys without deriving new identities.
            own[task_name] = task.content_hash
        if sorted(own) != dataset["task_names"] or len(own) != len(members):
            raise ValueError("published dataset does not match the job's task set")
        if set(own) & set(versions):
            raise ValueError("a job task belongs to more than one declared dataset")
        versions.update(own)
        sources.update({task_name: dataset["name"] for task_name in own})
    client = await create_authenticated_client()
    trials = []
    start = 0
    while True:
        page = (
            await client.table("trial")
            .select("id,task_name,task_content_hash,config")
            .eq("job_id", job_id)
            .order("id")
            .range(start, start + 999)
            .execute()
        ).data or []
        trials.extend(page)
        if len(page) < 1000:
            break
        start += 1000
    if (
        len(trials) != expected
        or {t["task_name"] for t in trials} != set(versions)
        or any(versions.get(t["task_name"]) != t["task_content_hash"] for t in trials)
    ):
        raise ValueError("uploaded trials do not match every published task version")
    response = (
        await client.table("job").select("config").eq("id", job_id).single().execute()
    )
    config = response.data["config"]
    by_path = {name.split("/", 1)[1]: source for name, source in sources.items()}
    for entry in config.get("tasks", []):
        name = Path(entry["path"]).name
        if name not in by_path:
            raise ValueError("job config refers to a task outside its dataset")
        entry["source"] = by_path[name]
    # The Hub job page reads datasets from the job config. This equivalent
    # definition pins the published tasks already checked above; retain the
    # execution's local paths separately so replay never selects both sources.
    if "tasks" in config:
        config["local_task_snapshots"] = config.pop("tasks")
    config["datasets"] = [
        {"name": d["name"], "ref": str(d["revision"])} for d in datasets
    ]
    config["hub_datasets"] = [
        {key: d[key] for key in ("name", "revision", "version_id")} for d in datasets
    ]
    semaphore = asyncio.Semaphore(12)

    async def label_trial(trial):
        task = trial["config"]["task"]
        source = sources[trial["task_name"]]
        if task.get("source") != source:
            task["source"] = source
            async with semaphore:
                await (
                    client.table("trial")
                    .update({"config": trial["config"]})
                    .eq("id", trial["id"])
                    .eq("job_id", job_id)
                    .execute()
                )

    await asyncio.gather(*(label_trial(trial) for trial in trials))
    await client.table("job").update({"config": config}).eq("id", job_id).execute()
    overview = await HubClient().get_job_overview([job_id])
    if set(overview.raw.get("datasets") or []) != {d["name"] for d in datasets}:
        raise RuntimeError("Harbor Hub has not confirmed the job's dataset links")
