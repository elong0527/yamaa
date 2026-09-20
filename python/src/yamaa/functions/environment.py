"""Resolve exactly one `environment.yaml` at one selected project root.

REQ-0663 gives the runner one explicitly selected root and REQ-0664 validates
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
    SharedFunctionContract,
    binding_arguments,
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

# The only language this runner can execute (REQ-0667).
RUNNER_LANGUAGE = "python"
_ENVIRONMENT_SCHEMA = "schema_environment.yaml"

_PYTHON_NAME = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
_PYTHON_CALL = re.compile(r"[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)+")
_R_NAME = re.compile(r"(?:[A-Za-z][A-Za-z0-9._]*|\.(?![0-9])[A-Za-z0-9._]+)")
_R_POSITIONAL = re.compile(r"\.\.[0-9]+")
_R_CALL = re.compile(r"[A-Za-z][A-Za-z0-9._]*:::?[A-Za-z._][A-Za-z0-9._]*")

# REQ-0683 refuses an R host argument name that names something else in the
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
    """Return the REQ-0695 failure every malformed declaration reports as."""
    return FunctionFailure(
        "project_environment_invalid",
        "REQ-0695",
        {"reason": reason, **context},  # type: ignore[arg-type]
    )


def environment_schema(schema_root: str | Path) -> SchemaBundle:
    """Load the bundle REQ-0664 validates a project environment against."""
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
    """Check what REQ-0676 and REQ-0683 require of one closed signature."""
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
    mapped = set(binding_arguments(contract))
    if mapped != seen:
        raise _invalid(
            "the binding must map the logical signature exactly",
            function=path,
            unmapped=sorted(seen - mapped),
            unknown=sorted(mapped - seen),
        )


def _check_binding(name: str, contract: FunctionContract, language: str) -> None:
    """Check the statically written callable and host names of REQ-0683."""
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
    host_names = list(binding_arguments(contract).values())
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


def _local_document_path(root: Path, written: str, *, field: str, name: str) -> Path:
    """Resolve a project-root-local document without leaving the root."""
    candidate = PurePosixPath(written)
    if candidate.is_absolute() or any(part in (".", "..") for part in candidate.parts):
        raise _invalid(
            f"a {field} path must be local and normalized",
            function=f"functions.{name}",
            **{field: written},  # type: ignore[arg-type]
        )
    resolved = root / Path(written)
    try:
        resolved.resolve().relative_to(root.resolve())
    except ValueError as error:
        raise _invalid(
            f"a {field} path must stay inside the project root",
            function=f"functions.{name}",
            **{field: written},  # type: ignore[arg-type]
        ) from error
    return resolved


def _conformance_path(root: Path, name: str, contract: FunctionContract) -> Path:
    """Resolve one vector document without leaving the project root."""
    return _local_document_path(
        root, contract.conformance, field="conformance", name=name
    )


# REQ-0669 names the language-neutral fields one shared contract document
# carries once for every project implementing the contract.
_SHARED_CONTRACT_FIELDS = (
    "contract_version",
    "description",
    "comparison_decimals",
    "may_return_missing",
    "params",
    "returns",
)


def _resolve_shared_contracts(
    document: dict[object, object],
    root: Path,
) -> None:
    """Merge REQ-0669 shared contracts into their function entries in place.

    Runs on the raw environment document before schema normalization, so a
    merged entry validates exactly like an inline contract. A `contract`
    reference and inline contract fields together are invalid, as is a
    reference to a document that does not define the referencing function.
    """
    functions = document.get("functions")
    if not isinstance(functions, dict):
        return
    for name, entry in functions.items():
        if not isinstance(entry, dict):
            continue
        reference = entry.get("contract")
        if reference is None:
            continue
        if not isinstance(reference, str):
            raise _invalid(
                "a shared contract reference must be a path",
                function=f"functions.{name}",
            )
        inline = [field for field in _SHARED_CONTRACT_FIELDS if field in entry]
        if inline:
            raise _invalid(
                "a function entry must declare its contract inline or name "
                "a shared contract, not both",
                function=f"functions.{name}",
                fields=inline,
            )
        path = _local_document_path(root, reference, field="contract", name=str(name))
        shared_document = _read_document(
            path,
            condition="project_environment_invalid",
            requirement="REQ-0695",
        )
        contracts: dict[str, SharedFunctionContract] = {}
        for contract_name, raw in shared_document.items():
            if not isinstance(raw, dict):
                raise _invalid(
                    "a shared contract document must map names to contracts",
                    function=f"functions.{name}",
                    contract=reference,
                    entry=contract_name,
                )
            # A shared contract is a schema fragment: it carries no
            # schema_version, so it validates against the pydantic model
            # directly. The schema bundle still checks every merged entry
            # structurally once the reference is resolved.
            try:
                validated = SharedFunctionContract.model_validate(raw, strict=True)
            except ValidationError as error:
                raise _invalid(
                    "a shared contract does not satisfy the model contract",
                    function=f"functions.{name}",
                    contract=reference,
                    entry=contract_name,
                    paths=[
                        ".".join(str(member) for member in item["loc"])
                        for item in error.errors(include_url=False, include_input=False)
                    ],
                ) from error
            contracts[str(contract_name)] = validated
        if str(name) not in contracts:
            raise _invalid(
                "a shared contract document must define the referencing function",
                function=f"functions.{name}",
                contract=reference,
            )
        merged = contracts[str(name)].model_dump(exclude_unset=True)
        merged.update({key: value for key, value in entry.items() if key != "contract"})
        functions[name] = merged


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
        requirement="REQ-0695",
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


def _default_implementation_versions(
    environment: ProjectEnvironment,
) -> ProjectEnvironment:
    """Apply the REQ-0669 default: an omitted implementation version is the
    environment version, so every later stage reads a plain string."""
    if all(
        contract.implementation_version is not None
        for contract in environment.functions.values()
    ):
        return environment
    return environment.model_copy(
        update={
            "functions": {
                name: contract
                if contract.implementation_version is not None
                else contract.model_copy(
                    update={"implementation_version": environment.version}
                )
                for name, contract in environment.functions.items()
            }
        }
    )


def load_environment(
    project_root: str | Path,
    schema_root: str | Path,
    *,
    bundle: SchemaBundle | None = None,
) -> LoadedEnvironment:
    """Load and validate the environment at one explicitly selected root.

    REQ-0664 fails here -- before any code is activated or executed -- when the
    environment is missing, unreadable, structurally invalid, or ambiguous.
    """
    root = Path(project_root)
    if not root.is_dir():
        raise FunctionFailure(
            "project_environment_missing",
            "REQ-0694",
            {"reason": "the selected project root is not a directory"},
        )
    schema = bundle if bundle is not None else environment_schema(schema_root)
    document = _read_document(
        root / ENVIRONMENT_NAME,
        condition="project_environment_missing",
        requirement="REQ-0694",
    )
    _resolve_shared_contracts(document, root)
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
    environment = _default_implementation_versions(environment)

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
        # REQ-0691 caches on the complete vector content, so the identity
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


def select_project_root(
    directory: Path,
    language: str = RUNNER_LANGUAGE,
) -> Path | None:
    """Return the project root a runner of `language` selects under here.

    REQ-0663 has the runner choose the root rather than the specification,
    and this is the rule this runner chooses by: a `<language>/` root when
    the directory offers one for the language the runner speaks, otherwise
    the directory itself when it holds an environment. One environment
    declares one language, which is why a benchmark demonstrating a contract
    in two of them keeps a directory per language. A directory with neither
    selects no project, which keeps the specification portable rather than
    broken.
    """
    candidate = directory / language
    if (candidate / ENVIRONMENT_NAME).is_file():
        return candidate
    if (directory / ENVIRONMENT_NAME).is_file():
        return directory
    return None


def check_runner_language(environment: ProjectEnvironment) -> None:
    """Reject a project whose runtime this runner cannot execute (REQ-0667)."""
    if environment.runtime.language != RUNNER_LANGUAGE:
        raise FunctionFailure(
            "runner_language_mismatch",
            "REQ-0696",
            {"runner": RUNNER_LANGUAGE, "declared": environment.runtime.language},
        )


__all__ = [
    "ENVIRONMENT_NAME",
    "RUNNER_LANGUAGE",
    "check_runner_language",
    "environment_schema",
    "load_environment",
    "select_project_root",
]
