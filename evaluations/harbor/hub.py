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
