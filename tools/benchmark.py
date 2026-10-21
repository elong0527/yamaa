"""Authored benchmark metadata shared by planning, validation and dashboards."""

import argparse
import re
import shutil
import tempfile
from pathlib import Path, PurePosixPath

import yaml

HOSTS = ("python", "r")
PROJECT = "project.yaml"
LIFECYCLES = {"draft": "lightgrey", "reviewed": "yellow", "finalized": "brightgreen"}
DEFINE_STANDARDS = ("sdtm", "adam", "send")
# The models a 1.0 specification declares (D25); `standard` may also name
# CDISC or TEST.
MODELS = ("SDTM", "ADaM", "SEND")
CREATED = re.compile(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}")


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
                "covers",
                "conformance",
                "harbor",
                "define",
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
    if "define" in metadata:
        define = metadata["define"]
        require(
            isinstance(define, dict)
            and set(define) == {"environment", "standard", "created"}
            and define["standard"] in DEFINE_STANDARDS
            and isinstance(define["created"], str)
            and CREATED.fullmatch(define["created"]),
            f"{label}: define declares its environment, standard and created time",
        )
        relative_file(case, define["environment"], "define environment")
    if "harbor" in metadata:
        harbor = metadata["harbor"]
        require(
            isinstance(harbor, dict)
            and set(harbor) == {"opensas_exclusion"}
            and isinstance(harbor["opensas_exclusion"], str)
            and harbor["opensas_exclusion"].strip(),
            f"{label}: Harbor exclusions must state their reason",
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
    """Preserve authored call order and the explicit project each call passes.

    A case's root `project.yaml` is the default for every call in both hosts;
    its `environments` key links each host's environment file. An entry's
    `environment` field names that project file, or is None for a call that
    opens its specification without a project.
    """
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
        default = PROJECT if (case / PROJECT).is_file() else None
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


def compare_define(case, staged, define):
    """Run the declared project submission over saved artifacts and compare it.

    `define` names the project file in its `environment` field and the
    submission section in its `standard` field. A positive case saves the
    submission and compares every file it writes, the Define-XML document and
    each transport file, with ``expected/`` byte for byte; a negative case
    compares its first issue with the partial assertion of
    ``expected/error.yaml``. Returns the problems.
    """
    import json

    import yamaa

    try:
        result = yamaa.project(staged / define["environment"]).submission(
            define["standard"], created=define["created"]
        )
        issues = result.issues.to_dicts()
    except yamaa.Error as error:
        result, issues = None, error.issues.to_dicts()
    for issue in issues:
        issue["context"] = json.loads(issue["context"])
    negative = case / "expected" / "error.yaml"
    if negative.exists():
        want = yaml.safe_load(negative.read_text(encoding="utf-8"))
        first = issues[0] if issues else None
        print(f"define first issue: {first}")

        def subset(expected, actual):
            if isinstance(expected, dict):
                return isinstance(actual, dict) and all(
                    key in actual and subset(value, actual[key])
                    for key, value in expected.items()
                )
            return expected == actual

        keys = ("phase", "condition", "spec_paths", "rule")
        if first is None or any(
            want.get(key) != first.get(key) for key in keys if key in want
        ):
            return [f"define first issue differs from expected/error.yaml: {first}"]
        if not subset(want.get("context", {}), first.get("context")):
            return [f"define issue context differs from expected/error.yaml: {first}"]
        return []
    if issues:
        return [f"define issues: {issues}"]

    def files():
        return {
            path.relative_to(staged).as_posix(): path.read_bytes()
            for path in staged.rglob("*")
            if path.is_file()
        }

    before = files()
    result.save()
    problems = []
    for name, content in sorted(files().items()):
        if before.get(name) == content:
            continue
        truth = case / "expected" / PurePosixPath(name).name
        verdict = (
            "matches"
            if truth.is_file() and truth.read_bytes() == content
            else "DIFFERS"
        )
        print(f"define {name}: {len(content)} bytes, {verdict} expected/{truth.name}")
        if verdict != "matches":
            problems.append(f"{name} differs from expected/{truth.name}")
    return problems


def layered_field(path, field):
    """The value of a root `field` in a specification's layers, or None.

    Layers contribute from the root ancestor down to the entry file, a later
    layer taking precedence (LAYER-3), so the layer nearest the entry that
    writes the field supplies it.
    """
    import yaml12

    layer, chain = Path(path), set()
    while True:
        identity = layer.resolve()
        require(identity not in chain, f"{path}: cyclic parent at {layer}")
        chain.add(identity)
        try:
            document = yaml12.parse_yaml(layer.read_text(encoding="utf-8"))
        except (OSError, UnicodeError, ValueError) as error:
            raise ValueError(f"{layer}: {error}") from error
        require(isinstance(document, dict), f"{layer}: expected a mapping")
        if field in document:
            return document[field]
        parent = document.get("parent")
        if parent is None:
            return None
        require(isinstance(parent, str) and parent, f"{layer}: parent must be one path")
        layer = layer.parent / parent


def specification_name(path):
    """The `domain` a project names a listed specification by (PROJECT-5)."""
    name = layered_field(path, "domain")
    require(
        isinstance(name, str) and bool(name),
        f"{path}: no layer names the specification's domain",
    )
    return name


def check_standard_model(case, metadata=None):
    """Require a benchmark's `standard` to be the model its entries declare.

    `standard` stays in benchmark.yaml, where the dashboards and the Harbor
    evaluation read it (#3030 Phase 1 and 2 plan, question 9). Where it names
    a model, the `model` that each host's last entry specification declares
    in the 1.0 surface (D25) is that model. A CDISC or TEST benchmark may
    build any model, and a 0.5 specification declares none. Layers a negative
    benchmark cannot compose leave its fault to the engine.
    """
    metadata = load_metadata(case) if metadata is None else metadata
    standard = metadata["standard"]
    if standard not in MODELS:
        return
    for calls in entries(case, metadata).values():
        specification = calls[-1]["specification"]
        try:
            model = layered_field(case / specification, "model")
        except ValueError:
            continue
        require(
            model in (None, standard),
            f"{case}: standard {standard} is not the model {model} that "
            f"{specification} declares",
        )


def main():
    """Build a case's declared Python entries in order inside a staged copy.

    Each entry is saved in the staged copy before the next entry is built, so a
    later entry reads an earlier entry's artifact at its own declared path, as
    a `schema` link requires. A case declaring ``define`` then runs that
    project submission over the saved artifacts and compares it with
    ``expected/``. Nothing is published into the checkout.
    """
    parser = argparse.ArgumentParser(description=main.__doc__)
    parser.add_argument("case", type=Path, help="benchmark directory")
    case = parser.parse_args().case.resolve()
    import yamaa

    from tools.staging.stage_project_inputs import stage_case

    metadata = load_metadata(case)
    calls = entries(case, metadata)["python"]
    define = metadata.get("define")
    with tempfile.TemporaryDirectory(prefix="yamaa-benchmark-") as temporary:
        staged = Path(temporary) / case.name
        stage_case(
            case,
            staged,
            ignore=shutil.ignore_patterns("expected", "README.md", "benchmark.yaml"),
        )
        for call in calls:
            specification = staged / call["specification"]
            project = call["environment"]
            if project is None:
                spec = yamaa.spec(specification)
            else:
                name = specification_name(specification)
                try:
                    spec = yamaa.project(staged / project).spec(name)
                except yamaa.Error as error:
                    # A project that fails admission refuses to name its specs.
                    parser.exit(1, f"{call['specification']}: {error.issues}\n")
            result = spec.run()
            output = result.output
            if output is None:
                parser.exit(1, f"{call['specification']}: {result.issues}\n")
            result.save()
            print(output)
        if define is not None:
            problems = compare_define(case, staged, define)
            if problems:
                parser.exit(1, "\n".join(problems) + "\n")


if __name__ == "__main__":
    main()
