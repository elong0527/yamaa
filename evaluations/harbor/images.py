"""Build language images and check isolation before and after Harbor setup.

    python evaluations/harbor/images.py build
    python evaluations/harbor/images.py verify --out /tmp/yamaa-image-checks

Verification uses Harbor's pinned OpenCode installer and runner, without a
remote model or credential. A local deterministic provider checks permissions.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import shlex
import subprocess
import uuid
from pathlib import Path
from types import SimpleNamespace

import build


class DockerCheck:
    """Run the installed-agent setup API against a disposable Docker container."""

    def __init__(self, name: str):
        self.name = name

    async def exec(self, command, *, user=None, env=None, cwd=None, timeout_sec=None):
        args = ["docker", "exec", "--user", user or "agent"]
        if cwd:
            args += ["--workdir", cwd]
        for key, value in (env or {}).items():
            args += ["--env", f"{key}={value}"]
        args += [self.name, "bash", "-c", command]
        process = await asyncio.create_subprocess_exec(
            *args, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE
        )
        try:
            stdout, stderr = await asyncio.wait_for(
                process.communicate(), timeout=timeout_sec or 900
            )
        except (TimeoutError, asyncio.CancelledError):
            process.kill()
            await process.communicate()
            raise
        return SimpleNamespace(
            return_code=process.returncode,
            stdout=stdout.decode(),
            stderr=stderr.decode(),
        )


def inventory_command(language: str, role: str) -> str:
    required = {build.LANGUAGES[language]["runner"]}
    if language == "r":
        required.add("R")
    if language == "python":
        required |= {"python", "python3"}
    if role == "verifier":
        required.add("python3")
        if language == "r":
            required.add("python")
    forbidden = set(build.INTERPRETER_COMMANDS) - required
    if language == "r":
        forbidden.discard("r")  # littler is optional, but belongs to the R track.
    checks = ["set -euo pipefail"]
    checks += [f"command -v {shlex.quote(p)} >/dev/null" for p in sorted(required)]
    checks += [
        f"if command -v {shlex.quote(p)} >/dev/null; then echo 'Unexpected runtime: {p}' >&2; exit 1; fi"
        for p in sorted(forbidden)
    ]
    if role == "agent":
        checks += ["node --version", "npm --version", "opencode --version"]
    else:
        checks += [
            f"if command -v {p} >/dev/null; then echo 'Unexpected agent infrastructure: {p}' >&2; exit 1; fi"
            for p in ("node", "npm", "opencode")
        ]
    if language == "opensas":
        checks += ['test "$(opensas --version)" = "opensas v0.6.6"']
    elif language == "python":
        checks += ["python3 -c 'import numpy, pandas, polars, pyarrow, lxml'"]
    else:
        checks += [
            'Rscript -e \'stopifnot(all(vapply(c("dplyr", "tidyr", "readr", "stringr", "lubridate", "purrr", "haven", "xml2", "nanoparquet"), requireNamespace, logical(1), quietly=TRUE)))\''
        ]
        if role == "verifier":
            checks += ["python3 -c 'import pyarrow'"]
    return "; ".join(checks)


async def verify_permissions(
    environment: DockerCheck, language: str, out: Path
) -> None:
    """Exercise explicit deny rules with the real runner's unattended flags."""
    from harbor.agents.installed.opencode import OpenCode
    from harbor.models.agent.context import AgentContext

    await asyncio.to_thread(
        subprocess.run,
        [
            "docker",
            "cp",
            str(build.HERE / "permission-probe.mjs"),
            f"{environment.name}:/tmp/permission-probe.mjs",
        ],
        check=True,
    )
    started = await environment.exec(
        f"node /tmp/permission-probe.mjs {build.LANGUAGES[language]['runner']} "
        "> /logs/agent/permission-api.txt 2>&1 < /dev/null &"
    )
    if started.return_code:
        raise RuntimeError(started.stderr)
    config = {
        "autoupdate": False,
        "share": "disabled",
        "lsp": False,
        "formatter": False,
        "snapshot": False,
        "permission": build.tool_permissions(language),
        "provider": {
            "yamaa-probe": {
                "npm": "@ai-sdk/openai-compatible",
                "name": "Local permission probe",
                "options": {
                    "baseURL": "http://127.0.0.1:18765/v1",
                    "apiKey": "local-test",
                },
                "models": {"probe": {"limit": {"context": 32000, "output": 1000}}},
            }
        },
    }
    agent = OpenCode(
        logs_dir=out,
        version=build.OPENCODE_VERSION,
        model_name="yamaa-probe/probe",
        opencode_config=config,
    )
    await asyncio.wait_for(
        agent.run("Exercise the permission probe tools.", environment, AgentContext()),
        timeout=120,
    )
    output = await environment.exec("cat /logs/agent/opencode.txt")
    (out / "opencode.txt").write_text(output.stdout)
    events = []
    for line in output.stdout.splitlines():
        try:
            events.append(json.loads(line))
        except ValueError:
            continue
    states = [e["part"]["state"] for e in events if e.get("type") == "tool_use"]
    if (
        len(states) != 2
        or states[0]["status"] != "error"
        or "rule which prevents" not in states[0].get("error", "")
        or states[1]["status"] != "completed"
        or states[1]["input"]["command"]
        != f"{build.LANGUAGES[language]['runner']} --version"
    ):
        raise RuntimeError(
            "OpenCode did not deny the helper call and allow the language runtime"
        )
    sentinel = await environment.exec("test ! -e /tmp/yamaa-foreign-runtime-ran")
    if sentinel.return_code:
        raise RuntimeError("a denied helper command executed")
    # Denied attempts must survive Harbor's conversion and remain ineligible.
    agent.populate_context_post_run(AgentContext())
    import grade

    audit = grade.scan_trajectory(out / "trajectory.json", language)
    if not audit["checked"] or len(audit["language_violations"]) != 1:
        raise RuntimeError(
            "the denied attempt was not retained in the trajectory audit"
        )
    (out / "permission-summary.json").write_text(json.dumps(audit, indent=2) + "\n")


async def verify(language: str, role: str, out: Path) -> dict:
    image = (build.AGENT_IMAGES if role == "agent" else build.VERIFIER_IMAGES)[language]
    name = f"yamaa-image-check-{role}-{language}-{uuid.uuid4().hex[:8]}"
    await asyncio.to_thread(
        subprocess.run,
        ["docker", "run", "-d", "--name", name, image, "sleep", "infinity"],
        check=True,
        stdout=subprocess.DEVNULL,
    )
    environment = DockerCheck(name)
    try:
        command = inventory_command(language, role)
        checks = out / f"{role}-{language}"
        checks.mkdir(parents=True, exist_ok=True)
        initial = await environment.exec(command)
        (checks / "before-setup.txt").write_text(initial.stdout + initial.stderr)
        if initial.return_code:
            raise RuntimeError(initial.stderr + initial.stdout)
        if role == "agent":
            from harbor.agents.installed.opencode import OpenCode

            agent = OpenCode(logs_dir=checks, version=build.OPENCODE_VERSION)
            await agent.install(environment)
            after = await environment.exec(command)
            (checks / "after-setup.txt").write_text(after.stdout + after.stderr)
            if after.return_code:
                raise RuntimeError(after.stderr + after.stdout)
            await verify_permissions(environment, language, checks)
        return {
            "language": language,
            "role": role,
            "image_reference": image,
            "passed": True,
            "harbor_setup_checked": role == "agent",
            "tool_permissions_checked": role == "agent",
        }
    finally:
        await asyncio.to_thread(
            subprocess.run,
            ["docker", "rm", "-f", name],
            check=True,
            stdout=subprocess.DEVNULL,
        )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["build", "verify"])
    parser.add_argument(
        "--language",
        nargs="+",
        choices=list(build.LANGUAGES),
        default=list(build.LANGUAGES),
    )
    parser.add_argument("--out", type=Path, default=build.OUT / "image-checks")
    args = parser.parse_args()
    if args.action == "build":
        for language in args.language:
            for role, images in (
                ("agent", build.AGENT_IMAGES),
                ("verifier", build.VERIFIER_IMAGES),
            ):
                subprocess.run(
                    [
                        "docker",
                        "build",
                        "--target",
                        f"{role}-{language}",
                        "-t",
                        images[language],
                        str(build.HERE),
                    ],
                    check=True,
                )
    else:
        args.out.mkdir(parents=True, exist_ok=True)
        results = []
        for language in args.language:
            for role in ("agent", "verifier"):
                try:
                    results.append(asyncio.run(verify(language, role, args.out)))
                except Exception as error:
                    results.append(
                        {
                            "language": language,
                            "role": role,
                            "passed": False,
                            "error": str(error),
                        }
                    )
                    raise
                finally:
                    (args.out / "summary.json").write_text(
                        json.dumps(results, indent=2) + "\n"
                    )
                print(f"verified {role}-{language}", flush=True)


if __name__ == "__main__":
    main()
