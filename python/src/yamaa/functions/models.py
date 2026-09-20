"""Strict models for one project function environment and its vectors.

REQ-0664 validates an environment independently of any specification, so these
models describe the environment document alone: one immutable runtime, the
logical contracts it implements, and the singular binding each contract has.
Nothing here reads a specification, resolves a callable, or runs a vector.
"""

from __future__ import annotations

from pathlib import Path
from typing import Literal, TypeAlias

from pydantic import BaseModel, ConfigDict, Field, JsonValue

from yamaa.specification.models import ColumnType

# REQ-0677 extends the R011 column vocabulary with `bool` for parameters only.
FunctionParamType: TypeAlias = Literal[
    "str", "int", "float", "bool", "date", "datetime"
]

RuntimeLanguage: TypeAlias = Literal["r", "python"]


class _StrictModel(BaseModel):
    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)


class FunctionParameter(_StrictModel):
    """One entry of a closed, ordered, named logical signature.

    REQ-0672 distinguishes an absent default from a present one, so `default`
    is read through `model_fields_set` rather than by comparing it to
    ``None``: a parameter defaulting to missing is not a parameter with no
    default.
    """

    name: str = Field(pattern=r"^[A-Za-z_][A-Za-z0-9_]*$")
    type: FunctionParamType
    required: bool = True
    default: JsonValue = None
    accepts_missing: bool = False

    @property
    def has_default(self) -> bool:
        return "default" in self.model_fields_set


class FunctionBinding(_StrictModel):
    """The one callable a project supplies for a logical contract."""

    call: str = Field(min_length=1)
    args: dict[str, str]


class FunctionContract(_StrictModel):
    """One logical contract and the singular binding implementing it."""

    contract_version: str = Field(min_length=1)
    implementation_version: str = Field(min_length=1)
    description: str = Field(min_length=1)
    comparison_decimals: int = 4
    may_return_missing: bool = False
    params: list[FunctionParameter]
    returns: ColumnType
    binding: FunctionBinding
    conformance: str = Field(min_length=1)

    @property
    def parameters(self) -> dict[str, FunctionParameter]:
        return {parameter.name: parameter for parameter in self.params}


class RuntimeArtifact(_StrictModel):
    """The immutable runtime REQ-0666 pins by verified content identity."""

    reference: str = Field(min_length=1)
    digest: str = Field(pattern=r"^sha256:[0-9a-f]{64}$")


class ProjectRuntime(_StrictModel):
    """One language and one artifact, shared by every binding (REQ-0665)."""

    language: RuntimeLanguage
    artifact: RuntimeArtifact


class ProjectEnvironment(_StrictModel):
    """One validated `environment.yaml` at a selected project root."""

    schema_version: str = Field(min_length=1)
    version: str = Field(min_length=1)
    runtime: ProjectRuntime
    functions: dict[str, FunctionContract]


class ConformanceCase(_StrictModel):
    """One named activation vector and the scalar it must produce."""

    id: str = Field(pattern=r"^[a-z][a-z0-9-]*$")
    covers: list[str]
    args: dict[str, JsonValue]
    result: JsonValue = None


class ConformanceDocument(_StrictModel):
    """The language-neutral vectors one contract is activated against."""

    schema_version: str = Field(min_length=1)
    function: str
    contract_version: str = Field(min_length=1)
    cases: list[ConformanceCase]


class LoadedEnvironment(_StrictModel):
    """One environment, its vectors, and the identities activation caches.

    REQ-0691 caches activation for the exact combination of environment
    version, artifact digest, every contract fingerprint, every
    implementation version, and the complete vector-content identity. The
    last two are read off this object, so nothing recomputes them from a
    document that may since have changed on disk.
    """

    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)

    root: Path
    environment: ProjectEnvironment
    conformance: dict[str, ConformanceDocument]
    fingerprints: dict[str, str]
    vector_identity: str = Field(pattern=r"^sha256:[0-9a-f]{64}$")


__all__ = [
    "ConformanceCase",
    "ConformanceDocument",
    "FunctionBinding",
    "FunctionContract",
    "FunctionParamType",
    "FunctionParameter",
    "LoadedEnvironment",
    "ProjectEnvironment",
    "ProjectRuntime",
    "RuntimeArtifact",
    "RuntimeLanguage",
]
