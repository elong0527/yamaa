"""CI validation for the shared portable function registry."""

from __future__ import annotations

import argparse
import math
import re
from pathlib import Path
from typing import Any, Iterable, Mapping, Sequence

import yaml


ENTRY_FIELDS = {
    "canonical_name",
    "aliases",
    "evaluation_kind",
    "signature",
    "type_promotion",
    "result_type",
    "missing_values",
    "failures",
    "determinism",
    "accuracy",
    "availability",
    "definition",
}
PARAMETER_TYPES = {
    "str",
    "int",
    "float",
    "bool",
    "date",
    "datetime",
    "record_star",
}
TYPE_PROMOTIONS = {
    "preserve_numeric",
    "promote_numeric",
    "always_float",
    "count",
    "preserve_input",
}
RESULT_TYPES = {"promoted_numeric", "input_numeric", "float", "int", "input"}
PROMOTION_RESULTS = {
    "preserve_numeric": {"promoted_numeric", "input_numeric"},
    "promote_numeric": {"promoted_numeric"},
    "always_float": {"float"},
    "count": {"int"},
    "preserve_input": {"input"},
}
MISSING_BEHAVIORS = {
    "propagate",
    "ignore_missing_all_missing",
    "first_non_missing",
    "null_if_equal",
    "count_non_missing_or_records",
}
DETERMINISM = {"binary64", "exact_or_binary64", "order_independent"}
ACCURACY_MODES = {
    "exact",
    "binary64",
    "exact_or_binary64",
    "absolute_or_relative",
}
NAME = re.compile(r"^[A-Z][A-Z0-9_]*$")
NAMESPACE = re.compile(r"^[a-z][a-z0-9_]*(?:\.[a-z][a-z0-9_]*)*$")
SEMVER = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+$")
SPEC_VERSION = re.compile(r"^[0-9]+\.[0-9]+$")


class PortableRegistryError(ValueError):
    """A stable portable registry validation failure."""

    def __init__(self, condition: str, **context: Any) -> None:
        self.condition = condition
        self.context = context
        details = ", ".join(f"{key}={value!r}" for key, value in context.items())
        super().__init__(f"{condition}: {details}" if details else condition)


class _StrictLoader(yaml.SafeLoader):
    pass


def _construct_mapping(
    loader: _StrictLoader, node: yaml.MappingNode, deep: bool = False
) -> dict[Any, Any]:
    mapping: dict[Any, Any] = {}
    for key_node, value_node in node.value:
        if key_node.value == "<<":
            raise PortableRegistryError("invalid_registry", reason="merge_key")
        key = loader.construct_object(key_node, deep=deep)
        if key in mapping:
            raise PortableRegistryError(
                "invalid_registry", reason="duplicate_key", key=key
            )
        mapping[key] = loader.construct_object(value_node, deep=deep)
    return mapping


_StrictLoader.add_constructor(
    yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, _construct_mapping
)


def _load_yaml(path: Path) -> Any:
    try:
        text = path.read_text(encoding="utf-8")
        for token in yaml.scan(text):
            if isinstance(token, (yaml.AnchorToken, yaml.AliasToken, yaml.TagToken)):
                raise PortableRegistryError(
                    "invalid_registry", path=str(path), reason="prohibited_yaml"
                )
        return yaml.load(text, Loader=_StrictLoader)
    except PortableRegistryError:
        raise
    except (OSError, yaml.YAMLError) as error:
        raise PortableRegistryError(
            "invalid_registry", path=str(path), reason=str(error)
        ) from error


def _require_fields(
    value: Any, expected: set[str], path: str, optional: set[str] | None = None
) -> Mapping[str, Any]:
    if not isinstance(value, dict):
        raise PortableRegistryError("invalid_registry", path=path, reason="not_mapping")
    optional = optional or set()
    actual = set(value)
    missing = expected - actual
    unknown = actual - expected - optional
    if missing or unknown:
        raise PortableRegistryError(
            "invalid_registry",
            path=path,
            missing=sorted(missing),
            unknown=sorted(unknown),
        )
    return value


def _version_key(version: str) -> tuple[int, ...]:
    return tuple(int(part) for part in version.split("."))


def _validate_document(document: Any, source: str) -> dict[str, Any]:
    document = dict(
        _require_fields(
            document,
            {"registry_version", "namespace", "specification_versions", "entries"},
            source,
        )
    )
    registry_version = document["registry_version"]
    namespace = document["namespace"]
    specification_versions = document["specification_versions"]
    entries = document["entries"]

    if not isinstance(registry_version, str) or not SEMVER.fullmatch(registry_version):
        raise PortableRegistryError(
            "invalid_registry", path=f"{source}.registry_version"
        )
    if not isinstance(namespace, str) or not NAMESPACE.fullmatch(namespace):
        raise PortableRegistryError("invalid_registry", path=f"{source}.namespace")
    if (
        not isinstance(specification_versions, list)
        or not specification_versions
        or any(
            not isinstance(version, str) or not SPEC_VERSION.fullmatch(version)
            for version in specification_versions
        )
        or len(set(specification_versions)) != len(specification_versions)
    ):
        raise PortableRegistryError(
            "invalid_registry", path=f"{source}.specification_versions"
        )
    if not isinstance(entries, list) or not entries:
        raise PortableRegistryError("invalid_registry", path=f"{source}.entries")

    names: dict[str, str] = {}
    validated_entries = []
    for index, entry_value in enumerate(entries):
        entry_path = f"{source}.entries[{index}]"
        entry = dict(_require_fields(entry_value, ENTRY_FIELDS, entry_path))
        canonical = entry["canonical_name"]
        aliases = entry["aliases"]
        if not isinstance(canonical, str) or not NAME.fullmatch(canonical):
            raise PortableRegistryError(
                "invalid_registry", path=f"{entry_path}.canonical_name"
            )
        if (
            not isinstance(aliases, list)
            or any(
                not isinstance(alias, str) or not NAME.fullmatch(alias)
                for alias in aliases
            )
            or len(set(aliases)) != len(aliases)
        ):
            raise PortableRegistryError(
                "invalid_registry", path=f"{entry_path}.aliases"
            )
        for candidate in [canonical, *aliases]:
            if candidate in names:
                raise PortableRegistryError(
                    "name_collision",
                    namespace=namespace,
                    name=candidate,
                    first=names[candidate],
                    second=canonical,
                )
            names[candidate] = canonical

        if entry["evaluation_kind"] not in {"scalar", "reducer"}:
            raise PortableRegistryError(
                "invalid_registry", path=f"{entry_path}.evaluation_kind"
            )
        _validate_signature(entry["signature"], entry_path)
        if entry["type_promotion"] not in TYPE_PROMOTIONS:
            raise PortableRegistryError(
                "invalid_registry", path=f"{entry_path}.type_promotion"
            )
        if entry["result_type"] not in RESULT_TYPES:
            raise PortableRegistryError(
                "invalid_registry", path=f"{entry_path}.result_type"
            )
        if entry["result_type"] not in PROMOTION_RESULTS[entry["type_promotion"]]:
            raise PortableRegistryError(
                "invalid_registry",
                path=f"{entry_path}.result_type",
                promotion=entry["type_promotion"],
            )
        if entry["missing_values"] not in MISSING_BEHAVIORS:
            raise PortableRegistryError(
                "invalid_registry", path=f"{entry_path}.missing_values"
            )
        if entry["determinism"] not in DETERMINISM:
            raise PortableRegistryError(
                "invalid_registry", path=f"{entry_path}.determinism"
            )
        if (
            not isinstance(entry["definition"], str)
            or not entry["definition"].strip()
        ):
            raise PortableRegistryError(
                "invalid_registry", path=f"{entry_path}.definition"
            )

        failures = _require_fields(
            entry["failures"],
            {"domain", "overflow", "non_finite_result"},
            f"{entry_path}.failures",
        )
        if (
            not isinstance(failures["domain"], list)
            or any(
                not isinstance(item, str)
                or not re.fullmatch(r"^[a-z][a-z0-9_]*$", item)
                for item in failures["domain"]
            )
            or len(set(failures["domain"])) != len(failures["domain"])
        ):
            raise PortableRegistryError(
                "invalid_registry", path=f"{entry_path}.failures.domain"
            )
        if failures["overflow"] != "fail" or failures["non_finite_result"] != "fail":
            raise PortableRegistryError(
                "invalid_registry", path=f"{entry_path}.failures"
            )

        accuracy = _require_fields(
            entry["accuracy"],
            {"mode", "absolute_tolerance", "relative_tolerance"},
            f"{entry_path}.accuracy",
        )
        if accuracy["mode"] not in ACCURACY_MODES:
            raise PortableRegistryError(
                "invalid_registry", path=f"{entry_path}.accuracy.mode"
            )
        for field in ("absolute_tolerance", "relative_tolerance"):
            if (
                isinstance(accuracy[field], bool)
                or not isinstance(accuracy[field], (int, float))
                or accuracy[field] < 0
                or not math.isfinite(accuracy[field])
            ):
                raise PortableRegistryError(
                    "invalid_registry", path=f"{entry_path}.accuracy.{field}"
                )

        availability = _require_fields(
            entry["availability"],
            {"since", "deprecated"},
            f"{entry_path}.availability",
        )
        if (
            not isinstance(availability["since"], str)
            or not SPEC_VERSION.fullmatch(availability["since"])
            or (
                availability["deprecated"] is not None
                and (
                    not isinstance(availability["deprecated"], str)
                    or not SPEC_VERSION.fullmatch(availability["deprecated"])
                )
            )
        ):
            raise PortableRegistryError(
                "invalid_registry", path=f"{entry_path}.availability"
            )
        validated_entries.append(entry)

    document["entries"] = validated_entries
    return document


def _validate_signature(signature_value: Any, entry_path: str) -> None:
    signature = _require_fields(
        signature_value,
        {"parameters", "min_arity", "max_arity"},
        f"{entry_path}.signature",
    )
    parameters = signature["parameters"]
    minimum = signature["min_arity"]
    maximum = signature["max_arity"]
    if not isinstance(parameters, list) or not parameters:
        raise PortableRegistryError(
            "invalid_registry", path=f"{entry_path}.signature.parameters"
        )
    if isinstance(minimum, bool) or not isinstance(minimum, int) or minimum < 1:
        raise PortableRegistryError(
            "invalid_registry", path=f"{entry_path}.signature.min_arity"
        )
    if maximum is not None and (
        isinstance(maximum, bool) or not isinstance(maximum, int) or maximum < minimum
    ):
        raise PortableRegistryError(
            "invalid_registry", path=f"{entry_path}.signature.max_arity"
        )

    variadic = False
    parameter_names = set()
    for index, parameter_value in enumerate(parameters):
        parameter_path = f"{entry_path}.signature.parameters[{index}]"
        parameter = _require_fields(
            parameter_value,
            {"name", "types"},
            parameter_path,
            optional={"variadic"},
        )
        name = parameter["name"]
        types = parameter["types"]
        if (
            not isinstance(name, str)
            or not re.fullmatch(r"^[a-z][a-z0-9_]*$", name)
            or name in parameter_names
        ):
            raise PortableRegistryError(
                "invalid_registry", path=f"{parameter_path}.name"
            )
        parameter_names.add(name)
        if (
            not isinstance(types, list)
            or not types
            or any(item not in PARAMETER_TYPES for item in types)
            or len(set(types)) != len(types)
        ):
            raise PortableRegistryError(
                "invalid_registry", path=f"{parameter_path}.types"
            )
        is_variadic = parameter.get("variadic", False)
        if not isinstance(is_variadic, bool) or (
            is_variadic and index != len(parameters) - 1
        ):
            raise PortableRegistryError(
                "invalid_registry", path=f"{parameter_path}.variadic"
            )
        variadic = variadic or is_variadic

    if variadic:
        if maximum is not None or minimum < len(parameters):
            raise PortableRegistryError(
                "invalid_registry", path=f"{entry_path}.signature"
            )
    elif minimum != len(parameters) or maximum != len(parameters):
        raise PortableRegistryError(
            "invalid_registry", path=f"{entry_path}.signature"
        )


class PortableRegistry:
    """Validated core registry and optional extension packs."""

    def __init__(
        self, core: Mapping[str, Any], extensions: Iterable[Mapping[str, Any]] = ()
    ) -> None:
        self.core = _validate_document(core, "core")
        if self.core["namespace"] != "core":
            raise PortableRegistryError(
                "invalid_registry", path="core.namespace", expected="core"
            )
        self.extensions: dict[str, dict[str, Any]] = {}
        for index, extension in enumerate(extensions):
            document = _validate_document(extension, f"extensions[{index}]")
            namespace = document["namespace"]
            if namespace == "core":
                raise PortableRegistryError(
                    "namespace_collision", namespace=namespace
                )
            if namespace in self.extensions:
                raise PortableRegistryError(
                    "namespace_collision", namespace=namespace
                )
            self.extensions[namespace] = document
        self._indexes = {
            "core": self._index(self.core),
            **{
                namespace: self._index(document)
                for namespace, document in self.extensions.items()
            },
        }

    @classmethod
    def load(
        cls, core_path: str | Path, extension_paths: Iterable[str | Path] = ()
    ) -> "PortableRegistry":
        return cls(
            _load_yaml(Path(core_path)),
            [_load_yaml(Path(path)) for path in extension_paths],
        )

    @staticmethod
    def _index(document: Mapping[str, Any]) -> dict[str, Mapping[str, Any]]:
        index = {}
        for entry in document["entries"]:
            for name in [entry["canonical_name"], *entry["aliases"]]:
                index[name] = entry
        return index

    def validate_call(
        self,
        name: str,
        evaluation_kind: str,
        argument_types: Sequence[str],
        specification_version: str,
        declared_extensions: Mapping[str, str] | None = None,
    ) -> Mapping[str, Any]:
        declared_extensions = declared_extensions or {}
        if "::" in name:
            parts = name.split("::")
            if len(parts) != 2 or not NAMESPACE.fullmatch(parts[0]):
                raise PortableRegistryError("unknown_function", name=name)
            namespace, local_name = parts
            required_version = declared_extensions.get(namespace)
            extension = self.extensions.get(namespace)
            if (
                required_version is None
                or extension is None
                or required_version != extension["registry_version"]
                or specification_version not in extension["specification_versions"]
            ):
                raise PortableRegistryError(
                    "unavailable_extension",
                    namespace=namespace,
                    required_version=required_version,
                )
        else:
            namespace = "core"
            local_name = name
            if specification_version not in self.core["specification_versions"]:
                raise PortableRegistryError(
                    "unavailable_function",
                    name=name,
                    specification_version=specification_version,
                )
        normalized = local_name.upper()
        entry = self._indexes[namespace].get(normalized)
        if entry is None:
            raise PortableRegistryError("unknown_function", name=name)
        if entry["evaluation_kind"] != evaluation_kind:
            raise PortableRegistryError(
                "wrong_evaluation_kind",
                name=name,
                expected=entry["evaluation_kind"],
                actual=evaluation_kind,
            )
        since = entry["availability"]["since"]
        if _version_key(specification_version) < _version_key(since):
            raise PortableRegistryError(
                "unavailable_function",
                name=name,
                specification_version=specification_version,
            )

        signature = entry["signature"]
        count = len(argument_types)
        maximum = signature["max_arity"]
        if count < signature["min_arity"] or (
            maximum is not None and count > maximum
        ):
            raise PortableRegistryError(
                "wrong_arity", name=name, actual=count,
                minimum=signature["min_arity"], maximum=maximum
            )
        parameters = signature["parameters"]
        for index, argument_type in enumerate(argument_types):
            parameter = parameters[min(index, len(parameters) - 1)]
            if argument_type != "null" and argument_type not in parameter["types"]:
                raise PortableRegistryError(
                    "incompatible_type",
                    name=name,
                    argument=index + 1,
                    parameter=parameter["name"],
                    actual=argument_type,
                    accepted=parameter["types"],
                )
        return entry


def check_portable_registry(
    registry_path: str | Path, fixtures_path: str | Path
) -> tuple[int, int]:
    registry_path = Path(registry_path)
    fixtures_path = Path(fixtures_path)
    fixture = _load_yaml(fixtures_path)
    _require_fields(
        fixture,
        {"registry_version", "evaluation_cases", "validation_cases"},
        str(fixtures_path),
    )
    registry = PortableRegistry.load(registry_path)
    if fixture["registry_version"] != registry.core["registry_version"]:
        raise AssertionError("fixture registry version does not match core registry")

    covered = set()
    for case in fixture["evaluation_cases"]:
        _require_fields(
            case,
            {
                "id",
                "name",
                "evaluation_kind",
                "argument_types",
                "arguments",
                "expected",
            },
            f"{fixtures_path}.{case.get('id', '<unknown>')}",
            optional={"specification_version"},
        )
        entry = registry.validate_call(
            case["name"],
            case["evaluation_kind"],
            case["argument_types"],
            case.get("specification_version", "1.0"),
        )
        if len(case["arguments"]) != len(case["argument_types"]):
            raise AssertionError(f"{case['id']}: argument values and types differ")
        covered.add(entry["canonical_name"])
    required = {entry["canonical_name"] for entry in registry.core["entries"]}
    if covered != required:
        raise AssertionError(f"fixture coverage differs: {sorted(required - covered)}")

    for case in fixture["validation_cases"]:
        extension_paths = [
            fixtures_path.parent / path for path in case.get("extension_registries", [])
        ]
        try:
            case_registry = PortableRegistry.load(registry_path, extension_paths)
            case_registry.validate_call(
                case["name"],
                case["evaluation_kind"],
                case["argument_types"],
                case.get("specification_version", "1.0"),
                case.get("declared_extensions", {}),
            )
        except PortableRegistryError as error:
            if error.condition != case["expected_error"]:
                raise AssertionError(
                    f"{case['id']}: expected {case['expected_error']}, "
                    f"got {error.condition}"
                ) from error
        else:
            if case.get("expected_error") is not None:
                raise AssertionError(f"{case['id']}: expected an error")
    return len(fixture["evaluation_cases"]), len(fixture["validation_cases"])


def render_documentation(registry: PortableRegistry) -> str:
    lines = [
        "# Portable function registry",
        "",
        "<!-- Generated by .github/workflows/check_portable_registry.py. -->",
        "",
        f"Registry version: `{registry.core['registry_version']}`",
        "",
    ]
    for kind, heading in (("scalar", "Scalar functions"), ("reducer", "Reducers")):
        lines.extend(
            [
                f"## {heading}",
                "",
                "| Name | Aliases | Arity | Parameters | Result | Since |",
                "|---|---|---:|---|---|---|",
            ]
        )
        for entry in registry.core["entries"]:
            if entry["evaluation_kind"] != kind:
                continue
            signature = entry["signature"]
            maximum = signature["max_arity"]
            arity = (
                str(signature["min_arity"])
                if maximum == signature["min_arity"]
                else f"{signature['min_arity']}..{maximum or '*'}"
            )
            parameters = "; ".join(
                f"{parameter['name']}: {'/'.join(parameter['types'])}"
                + ("..." if parameter.get("variadic") else "")
                for parameter in signature["parameters"]
            )
            aliases = ", ".join(entry["aliases"]) or "-"
            lines.append(
                f"| `{entry['canonical_name']}` | {aliases} | {arity} | "
                f"{parameters} | `{entry['result_type']}` | "
                f"{entry['availability']['since']} |"
            )
        lines.append("")
    return "\n".join(lines)


def main(argv: Sequence[str] | None = None) -> int:
    root = Path(__file__).resolve().parents[2]
    default_registry = root / "yaml/registry/portable-functions.yaml"
    default_fixtures = root / "yaml/registry/conformance.yaml"
    default_docs = root / "yaml/registry/README.md"
    parser = argparse.ArgumentParser()
    parser.add_argument("--registry", type=Path, default=default_registry)
    parser.add_argument("--fixtures", type=Path, default=default_fixtures)
    parser.add_argument("--write-doc", type=Path)
    parser.add_argument("--check-doc", type=Path)
    args = parser.parse_args(argv)

    evaluation_count, validation_count = check_portable_registry(
        args.registry, args.fixtures
    )
    registry = PortableRegistry.load(args.registry)
    rendered = render_documentation(registry)
    if args.write_doc:
        args.write_doc.write_text(rendered, encoding="utf-8")
    if args.check_doc:
        actual = args.check_doc.read_text(encoding="utf-8")
        if actual != rendered:
            raise SystemExit(f"generated documentation differs: {args.check_doc}")
    print(
        f"Portable registry validated {evaluation_count} evaluation contracts "
        f"and {validation_count} validation fixtures."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
