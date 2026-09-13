"""Resolve exactly one `environment.yaml` at one selected project root.

R018-2 gives the runner one explicitly selected root and R018-3 validates
what it finds there independently of any specification: a specification can
neither name this file nor change what it says. Everything this module
produces -- the contracts, their fingerprints, and the vector documents --
is read before a single callable is resolved, so a project whose environment
does not describe itself correctly never reaches its own code.
"""

from __future__ import annotations

import hashlib
import keyword
import re
from pathlib import Path, PurePosixPath

from pydantic import ValidationError

from yamaa.functions.errors import FunctionFailure
from yamaa.functions.fingerprint import (
    ContractValueError,
    contract_fingerprint,
    function_value_type,
)
from yamaa.functions.models import (
    ConformanceDocument,
    FunctionContract,
    LoadedEnvironment,
    ProjectEnvironment,
)
from yamaa.specification._yaml import read_yaml_document
from yamaa.specification.diagnostics import SpecificationError
from yamaa.specification.schema import (
    SchemaBundle,
    load_schema_bundle,
    normalize_document,
    validate_document,
)

ENVIRONMENT_NAME = "environment.yaml"
_ENVIRONMENT_SCHEMA = "schema_environment.yaml"

_PYTHON_NAME = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
_PYTHON_CALL = re.compile(r"[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)+")
_R_NAME = re.compile(r"(?:[A-Za-z][A-Za-z0-9._]*|\.(?![0-9])[A-Za-z0-9._]+)")
_R_POSITIONAL = re.compile(r"\.\.[0-9]+")
_R_CALL = re.compile(r"[A-Za-z][A-Za-z0-9._]*:::?[A-Za-z._][A-Za-z0-9._]*")

# R018-22 refuses an R host argument name that names something else in the
# language the binding is written in.
_R_RESERVED = frozenset(
    {
        "break",
        "else",
        "FALSE",
        "for",
        "function",
        "if",
        "Inf",
        "in",
        "NA",
        "NA_character_",
        "NA_complex_",
        "NA_integer_",
        "NA_real_",
        "NaN",
        "next",
        "NULL",
        "repeat",
        "TRUE",
        "while",
    }
)


def _invalid(reason: str, **context: object) -> FunctionFailure:
    """Return the R018-34 failure every malformed declaration reports as."""
    return FunctionFailure(
        "project_environment_invalid",
        "R018-34",
        {"reason": reason, **context},  # type: ignore[arg-type]
    )


def environment_schema(schema_root: str | Path) -> SchemaBundle:
    """Load the bundle R018-3 validates a project environment against."""
    return load_schema_bundle(
        schema_root,
        entry_name=_ENVIRONMENT_SCHEMA,
        root_class="environment_class",
    )


def _read_document(
    path: Path,
    *,
    condition: str,
    requirement: str,
) -> dict[object, object]:
    if path.is_symlink() or not path.is_file():
        raise FunctionFailure(condition, requirement, {"document": path.name})
    try:
        document = read_yaml_document(path)
    except OSError as error:
        raise FunctionFailure(
            condition,
            requirement,
            {
                "document": path.name,
                "reason": "document could not be read",
                "host_error": type(error).__name__,
                "host_message": str(error),
            },
        ) from error
    except SpecificationError as error:
        raise _invalid(
            "document could not be read",
            document=path.name,
            conditions=[item.condition for item in error.diagnostics],
        ) from error
    if not isinstance(document, dict):
        raise _invalid("document must be a mapping", document=path.name)
    return document


def _model(
    document: dict[object, object],
    bundle: SchemaBundle,
    class_name: str,
    model: type[ProjectEnvironment | ConformanceDocument],
    label: str,
) -> ProjectEnvironment | ConformanceDocument:
    diagnostics = validate_document(document, bundle, class_name)
    if diagnostics:
        raise _invalid(
            "document does not satisfy the environment schema",
            document=label,
            paths=[path for item in diagnostics for path in item.spec_paths],
            conditions=[item.condition for item in diagnostics],
        )
    normalized = normalize_document(document, bundle, class_name)
    try:
        return model.model_validate(normalized, strict=True)
    except ValidationError as error:
        raise _invalid(
            "document does not satisfy the model contract",
            document=label,
            paths=[
                ".".join(str(member) for member in item["loc"])
                for item in error.errors(include_url=False, include_input=False)
            ],
        ) from error


def _check_signature(name: str, contract: FunctionContract) -> None:
    """Check what R018-15 and R018-22 require of one closed signature."""
    path = f"functions.{name}"
    seen: set[str] = set()
    for parameter in contract.params:
        if parameter.name in seen:
            raise _invalid("parameter names must be unique", function=path)
        seen.add(parameter.name)
        if parameter.required and parameter.has_default:
            raise _invalid(
                "a required parameter cannot declare a default",
                function=path,
                parameter=parameter.name,
            )
        if not parameter.required and not parameter.has_default:
            raise _invalid(
                "an optional parameter requires an environment default",
                function=path,
                parameter=parameter.name,
            )
        default_type = function_value_type(parameter.default)
        if parameter.has_default and not (
            default_type == parameter.type
            or (default_type is None and parameter.accepts_missing)
        ):
            raise _invalid(
                "a default must carry its declared exact type",
                function=path,
                parameter=parameter.name,
                expected=parameter.type,
                actual=default_type,
            )
    mapped = set(contract.binding.args)
    if mapped != seen:
        raise _invalid(
            "the binding must map the logical signature exactly",
            function=path,
            unmapped=sorted(seen - mapped),
            unknown=sorted(mapped - seen),
        )


def _check_binding(name: str, contract: FunctionContract, language: str) -> None:
    """Check the statically written callable and host names of R018-22."""
    path = f"functions.{name}"
    call = contract.binding.call
    pattern = _PYTHON_CALL if language == "python" else _R_CALL
    if pattern.fullmatch(call) is None:
        raise _invalid(
            "the binding call must be a qualified callable of the runtime language",
            function=path,
            call=call,
            language=language,
        )
    host_names = list(contract.binding.args.values())
    if len(set(host_names)) != len(host_names):
        raise _invalid(
            "each logical parameter needs one unique host argument name",
            function=path,
        )
    for host_name in host_names:
        if not _valid_host_name(language, host_name):
            raise _invalid(
                "a host argument name must be a name of the runtime language",
                function=path,
                host_argument=host_name,
                language=language,
            )


def _valid_host_name(language: str, name: str) -> bool:
    if language == "python":
        return _PYTHON_NAME.fullmatch(name) is not None and not keyword.iskeyword(name)
    return (
        _R_NAME.fullmatch(name) is not None
        and name not in _R_RESERVED
        and name != "..."
        and _R_POSITIONAL.fullmatch(name) is None
    )


def _conformance_path(root: Path, name: str, contract: FunctionContract) -> Path:
    """Resolve one vector document without leaving the project root."""
    written = PurePosixPath(contract.conformance)
    if written.is_absolute() or any(part in (".", "..") for part in written.parts):
        raise _invalid(
            "a conformance path must be local and normalized",
            function=f"functions.{name}",
            conformance=contract.conformance,
        )
    candidate = root / Path(contract.conformance)
    try:
        candidate.resolve().relative_to(root.resolve())
    except ValueError as error:
        raise _invalid(
            "a conformance path must stay inside the project root",
            function=f"functions.{name}",
            conformance=contract.conformance,
        ) from error
    return candidate


def _load_conformance(
    root: Path,
    name: str,
    contract: FunctionContract,
    bundle: SchemaBundle,
) -> tuple[ConformanceDocument, bytes]:
    path = _conformance_path(root, name, contract)
    document = _read_document(
        path,
        condition="project_environment_invalid",
        requirement="R018-34",
    )
    vectors = _model(
        document,
        bundle,
        "function_conformance_class",
        ConformanceDocument,
        contract.conformance,
    )
    assert isinstance(vectors, ConformanceDocument)
    if (
        vectors.function != name
        or vectors.contract_version != contract.contract_version
    ):
        raise _invalid(
            "a vector document must identify its own contract",
            function=f"functions.{name}",
            declared=[vectors.function, vectors.contract_version],
            expected=[name, contract.contract_version],
        )
    identifiers = [case.id for case in vectors.cases]
    if not identifiers:
        raise _invalid(
            "a vector document requires at least one case",
            function=f"functions.{name}",
        )
    if len(set(identifiers)) != len(identifiers):
        raise _invalid("case names must be unique", function=f"functions.{name}")
    return vectors, path.read_bytes()


def load_environment(
    project_root: str | Path,
    schema_root: str | Path,
    *,
    bundle: SchemaBundle | None = None,
) -> LoadedEnvironment:
    """Load and validate the environment at one explicitly selected root.

    R018-3 fails here -- before any code is activated or executed -- when the
    environment is missing, unreadable, structurally invalid, or ambiguous.
    """
    root = Path(project_root)
    if not root.is_dir():
        raise FunctionFailure(
            "project_environment_missing",
            "R018-33",
            {"reason": "the selected project root is not a directory"},
        )
    schema = bundle if bundle is not None else environment_schema(schema_root)
    document = _read_document(
        root / ENVIRONMENT_NAME,
        condition="project_environment_missing",
        requirement="R018-33",
    )
    environment = _model(
        document,
        schema,
        "environment_class",
        ProjectEnvironment,
        ENVIRONMENT_NAME,
    )
    assert isinstance(environment, ProjectEnvironment)
    if not environment.functions:
        raise _invalid("an environment must declare at least one function")

    fingerprints: dict[str, str] = {}
    conformance: dict[str, ConformanceDocument] = {}
    digest = hashlib.sha256()
    for name, contract in sorted(environment.functions.items()):
        _check_signature(name, contract)
        _check_binding(name, contract, environment.runtime.language)
        try:
            fingerprints[name] = contract_fingerprint(name, contract)
        except (ContractValueError, ValueError) as error:
            raise _invalid(
                "a contract fingerprint could not be calculated",
                function=f"functions.{name}",
                detail=str(error),
            ) from error
        vectors, content = _load_conformance(root, name, contract, schema)
        conformance[name] = vectors
        # R018-30 caches on the complete vector content, so the identity
        # covers the bytes of every document rather than its declared path.
        digest.update(f"{name}\n{len(content)}\n".encode())
        digest.update(content)

    return LoadedEnvironment(
        root=root,
        environment=environment,
        conformance=conformance,
        fingerprints=fingerprints,
        vector_identity=f"sha256:{digest.hexdigest()}",
    )


def check_runner_language(environment: ProjectEnvironment) -> None:
    """Reject a project whose runtime this runner cannot execute (R018-6)."""
    if environment.runtime.language != "python":
        raise FunctionFailure(
            "runner_language_mismatch",
            "R018-35",
            {"runner": "python", "declared": environment.runtime.language},
        )


__all__ = [
    "ENVIRONMENT_NAME",
    "check_runner_language",
    "environment_schema",
    "load_environment",
]
