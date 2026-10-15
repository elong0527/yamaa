"""Authored benchmark metadata shared by planning, validation and dashboards."""

import argparse
import re
from pathlib import Path, PurePosixPath

import yaml

HOSTS = ("python", "r")
LIFECYCLES = {"draft": "lightgrey", "reviewed": "yellow", "finalized": "brightgreen"}


class MetadataLoader(yaml.SafeLoader):
    """Reject duplicate metadata keys rather than silently replacing entries."""


def unique_mapping(loader, node):
    pairs = loader.construct_pairs(node, deep=True)
    result = {}
    for key, value in pairs:
        if not isinstance(key, str) or key in result:
            raise ValueError("benchmark metadata needs unique string keys")
        result[key] = value
    return result


MetadataLoader.add_constructor(
    yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, unique_mapping
)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def load_metadata(case):
    """Read the sole metadata owner; README text never supplies configuration."""
    path = case / "benchmark.yaml"
    try:
        metadata = yaml.load(path.read_text(encoding="utf-8"), Loader=MetadataLoader)
    except (OSError, UnicodeError, yaml.YAMLError, ValueError) as error:
        raise ValueError(f"{path}: {error}") from error
    label = str(path)
    require(isinstance(metadata, dict), f"{label}: expected a mapping")
    require(
        not (
            set(metadata)
            - {
                "lifecycle",
                "standard",
                "domain",
                "entries",
                "validation",
                "covers",
                "conformance",
            }
        ),
        f"{label}: unknown metadata field",
    )
    require(
        isinstance(metadata.get("lifecycle"), str)
        and metadata["lifecycle"] in LIFECYCLES,
        f"{label}: invalid lifecycle",
    )
    for field in ("standard", "domain"):
        require(
            isinstance(metadata.get(field), str)
            and metadata[field].strip() == metadata[field]
            and metadata[field],
            f"{label}: {field} must be nonempty text",
        )
    covers = metadata.get("covers", [])
    require(
        isinstance(covers, list)
        and all(
            isinstance(operation, str)
            and re.fullmatch(r"(?:verify:)?[a-z][a-z0-9_]*", operation)
            for operation in covers
        )
        and len(set(covers)) == len(covers),
        f"{label}: covers must name unique operations",
    )
    if "validation" in metadata:
        validation = metadata["validation"]
        require(
            isinstance(validation, dict)
            and set(validation) in ({"rule", "validator"}, {"rule", "blocked_by"})
            and isinstance(validation.get("rule"), str)
            and re.fullmatch(r"R[0-9]{3}", validation["rule"]),
            f"{label}: validation needs rule and exactly one validator or blocked_by",
        )
        key = "validator" if "validator" in validation else "blocked_by"
        pattern = (
            r"[a-z][a-z0-9]*(?:_[a-z0-9]+)*" if key == "validator" else r"#[1-9][0-9]*"
        )
        require(
            isinstance(validation[key], str) and re.fullmatch(pattern, validation[key]),
            f"{label}: invalid validation {key}",
        )
    if "conformance" in metadata:
        conformance = metadata["conformance"]
        require(
            isinstance(conformance, dict)
            and not (set(conformance) - {"excluded", "metamorphic"}),
            f"{label}: invalid conformance metadata",
        )
        if "excluded" in conformance:
            require(
                isinstance(conformance["excluded"], str)
                and conformance["excluded"].strip(),
                f"{label}: exclusion must state its reason",
            )
        differences = conformance.get("metamorphic", {})
        require(
            isinstance(differences, dict), f"{label}: metamorphic must be a mapping"
        )
        for check, declaration in differences.items():
            require(
                check in {"empty_child", "key_order"}
                and isinstance(declaration, dict)
                and not (set(declaration) - {"reason", "issue", "permanent"})
                and type(declaration.get("permanent", False)) is bool
                and ("reason" in declaration or "issue" in declaration)
                and all(
                    isinstance(declaration[field], str) and declaration[field].strip()
                    for field in ("reason", "issue")
                    if field in declaration
                )
                and (not declaration.get("permanent") or "reason" in declaration),
                f"{label}: malformed metamorphic declaration: {check}",
            )
    return metadata


def directories(fixtures):
    """Find authored cases, retaining missing-metadata/specification errors."""
    return [
        case
        for case in sorted(fixtures.iterdir())
        if case.is_dir()
        and ((case / "benchmark.yaml").exists() or list(case.glob("spec*.yaml")))
    ]


def relative_file(case, value, field):
    require(
        isinstance(value, str) and value and "\\" not in value,
        f"{case}: {field} must be a relative file",
    )
    path = PurePosixPath(value)
    require(
        not path.is_absolute() and ".." not in path.parts and str(path) == value,
        f"{case}: {field} must stay inside the benchmark",
    )
    require((case / value).is_file(), f"{case}: missing {field}: {value}")
    return value


def entries(case, metadata=None):
    """Preserve authored call order and explicit per-host environments."""
    metadata = load_metadata(case) if metadata is None else metadata
    declared = metadata.get("entries")
    if "entries" not in metadata:
        specs = sorted(case.glob("spec*.yaml"))
        require(
            len(specs) == 1, f"{case}: multiple specifications need explicit entries"
        )
        declared = {host: [{"specification": specs[0].name}] for host in HOSTS}
    require(
        isinstance(declared, dict) and set(declared) == set(HOSTS),
        f"{case}: entries must declare Python and R",
    )
    result = {}
    for host in HOSTS:
        calls = declared[host]
        require(
            isinstance(calls, list) and calls,
            f"{case}: {host} entries must be a nonempty list",
        )
        result[host] = []
        default = f"{host}/environment.yaml"
        default = default if (case / default).is_file() else None
        for call in calls:
            require(
                isinstance(call, dict)
                and "specification" in call
                and not (set(call) - {"specification", "environment"}),
                f"{case}: malformed {host} entry",
            )
            specification = relative_file(case, call["specification"], "specification")
            require(
                specification.endswith(".yaml"), f"{case}: specification must be YAML"
            )
            environment = call.get("environment", default)
            if environment is not None:
                relative_file(case, environment, "environment")
            value = {"specification": specification, "environment": environment}
            require(value not in result[host], f"{case}: duplicate {host} entry")
            result[host].append(value)
    return result


def main():
    """Run a case's declared Python entries without publishing output files."""
    parser = argparse.ArgumentParser(description=main.__doc__)
    parser.add_argument("case", type=Path, help="benchmark directory")
    case = parser.parse_args().case.resolve()
    from yamaa import domain

    for call in entries(case)["python"]:
        environment = call["environment"]
        result = domain(
            case / call["specification"],
            environment=None if environment is None else case / environment,
        )
        output = result.output
        if output is None:
            parser.exit(1, f"{call['specification']}: {result.issues}\n")
        print(output)


if __name__ == "__main__":
    main()
