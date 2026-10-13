"""Development fixture observations from typed core, never installed APIs.

These examples serve independently authored grammar and partial-AST assertions.
A parsed fixture grants no source binding or execution capability.
"""

import json
import os
import subprocess
from functools import lru_cache
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
EXAMPLES = {
    name: name.replace("-", "_") + "_syntax_fixture"
    for name in ("predicate", "numeric", "aggregate", "string-template")
}
EXAMPLES["windows"] = "window_fixture"


@lru_cache(maxsize=1)
def _binaries():
    command = ["cargo", "build", "--locked", "--quiet", "-p", "yamaa-adapters"]
    for example in EXAMPLES.values():
        command.extend(["--example", example])
    subprocess.run(command, cwd=ROOT / "rust", check=True)
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "rust" / "target"))
    if not target.is_absolute():
        target = ROOT / "rust" / target
    suffix = ".exe" if os.name == "nt" else ""
    return {
        name: target / "debug" / "examples" / (example + suffix)
        for name, example in EXAMPLES.items()
    }


@lru_cache(maxsize=16384)
def _observation(contract, text, strict=True):
    result = subprocess.run(
        [
            str(_binaries()[contract]),
            *(["--spans"] if contract in {"numeric", "aggregate"} else []),
            *(["--non-strict"] if contract == "windows" and not strict else []),
        ],
        input=json.dumps(text) + "\n",
        text=True,
        capture_output=True,
        check=True,
    )
    rows = result.stdout.splitlines()
    if len(rows) != 1:
        raise RuntimeError("incomplete typed grammar fixture observation")
    value = json.loads(rows[0])
    return value if contract == "windows" else value["outcome"]


def observe(contract, text, *, strict=True):
    # Consumers attach spans and diagnostic prose; protect the cached typed fact.
    return json.loads(json.dumps(_observation(contract, text, strict)))


def spans(value):
    if isinstance(value, dict):
        if value.get("kind") == "star":
            value["kind"] = "qualified_star"
        for key, child in value.items():
            if key.endswith("span") and isinstance(child, list):
                value[key] = tuple(child)
            else:
                spans(child)
    elif isinstance(value, list):
        for child in value:
            spans(child)
    return value
