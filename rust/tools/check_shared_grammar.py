"""Replay shared YAML shape, identifier, decision and vocabulary truth in typed core."""

import importlib.util
import subprocess
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "stage_grammar_inputs", Path(__file__).with_name("stage_grammar_inputs.py")
)
grammar = importlib.util.module_from_spec(spec)
spec.loader.exec_module(grammar)


def main():
    contracts = list(grammar.cases())
    docs = dict(grammar.documents())
    inventories = [
        (
            "numeric-vocabulary",
            {
                "id": "closed-functions",
                "text": "",
                "parse": "accept",
                "identifiers": list(docs["numeric"]["vocabulary"]["function"]),
            },
        ),
        (
            "aggregate-vocabulary",
            {
                "id": "closed-reducers",
                "text": "",
                "parse": "accept",
                "identifiers": list(docs["aggregate"]["vocabulary"]["reducer"]),
            },
        ),
    ]
    contracts = inventories + contracts
    result = subprocess.run(
        [
            "cargo",
            "run",
            "--locked",
            "--quiet",
            "-p",
            "yamaa-core",
            "--example",
            "grammar_contract",
        ],
        cwd=Path(__file__).resolve().parents[1],
        input="".join(
            contract + "\t" + case["text"].encode().hex() + "\n"
            for contract, case in contracts
        ),
        text=True,
        capture_output=True,
        check=True,
    )
    rows = result.stdout.splitlines()
    if len(rows) != len(contracts):
        raise AssertionError("incomplete shared grammar replay")
    for (contract, case), row in zip(contracts, rows, strict=True):
        fields = row.split("\t")
        label = contract + "/" + case["id"]
        if fields[0] != case["parse"]:
            raise AssertionError((label, case, row))
        if fields[0] == "reject":
            if fields[1] != case["condition"]:
                raise AssertionError((label, case["condition"], fields[1]))
        else:
            if "shape" in case and bytes.fromhex(fields[1]).decode() != " ".join(
                case["shape"].split()
            ):
                raise AssertionError(
                    (label, case["shape"], bytes.fromhex(fields[1]).decode())
                )
            if "identifiers" in case:
                names = (
                    bytes.fromhex(fields[2]).decode().split(",") if fields[2] else []
                )
                if names != sorted(case["identifiers"]):
                    raise AssertionError((label, case["identifiers"], names))
    print(
        f"{len(contracts)} shared grammar decision, shape, dependency and vocabulary checks passed"
    )


if __name__ == "__main__":
    main()
