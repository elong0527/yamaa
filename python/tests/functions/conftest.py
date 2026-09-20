"""Build project roots a test can pin, break, and re-pin.

Every root these fixtures write is a real one: an `environment.yaml`, the
vectors it names, and a runtime directory holding the code its binding
resolves. The digest is substituted after the code is written, so a test
that changes one byte of that code and re-pins exercises the same path a
project publishing a new artifact does.
"""

from __future__ import annotations

import sys
from dataclasses import dataclass
from pathlib import Path

import pytest

from yamaa.functions import ACTIVATION_CACHE, artifact_digest

_REPOSITORY_ROOT = Path(__file__).parents[3]

BMI_CODE = """
def bmi(weight_kg, height_cm, cm_per_m=100):
    return weight_kg / (height_cm / cm_per_m) ** 2
"""

# The same arithmetic, with every invocation recorded where a test can read
# it. REQ-0691 is an ordering rule, and an order is only observable from
# inside the code that gets called.
RECORDING_CODE = """
CALLS = []


def bmi(weight_kg, height_cm, cm_per_m=100):
    CALLS.append((weight_kg, height_cm, cm_per_m))
    return weight_kg / (height_cm / cm_per_m) ** 2
"""

ENVIRONMENT = """schema_version: "1.0"
version: "{version}"

runtime:
  language: {language}
  artifact:
    reference: org.example/yamaa/bmi-python:1.0.0
    digest: {digest}

functions:
  bmi:
    contract_version: "{contract_version}"
    implementation_version: "1.0.0"
    description: Calculate body mass index from kilograms and centimetres.
    comparison_decimals: 4
    may_return_missing: {may_return_missing}
    params:
      - name: weight_kg
        type: float
        accepts_missing: false
      - name: height_cm
        type: float
        accepts_missing: {accepts_missing}
      - name: cm_per_m
        type: int
        required: false
        default: 100
        accepts_missing: false
    returns: {returns}
    binding:
      call: {call}
      args:
        weight_kg: weight_kg
        height_cm: height_cm
        cm_per_m: cm_per_m
    conformance: conformance/bmi.yaml
"""

ENVIRONMENT_DEFAULTS = {
    "version": "1.0.0",
    "contract_version": "1.0.0",
    "language": "python",
    "call": "projectbmi.bmi",
    "returns": "float",
    "may_return_missing": "false",
    "accepts_missing": "false",
}


@dataclass(frozen=True, slots=True)
class Repository:
    """The committed files these tests read rather than write."""

    root: Path

    @property
    def schema(self) -> Path:
        return self.root / "yaml"

    @property
    def examples(self) -> Path:
        return self.root / "benchmarks"

    @property
    def bmi_example(self) -> Path:
        """The committed example, whose project root implements `bmi` in R."""
        return self.examples / "adam-adsl-bmi-function"

    @property
    def bmi_project(self) -> Path:
        """The Python project root implementing that same logical contract."""
        return self.root / "python/tests/projects/bmi-python"

    @property
    def validator(self) -> Path:
        return self.root / ".github/scripts/yaml-validation/validate_repository.py"

    @property
    def vectors(self) -> str:
        return (self.bmi_project / "conformance/bmi.yaml").read_text("utf-8")


@dataclass(frozen=True, slots=True)
class ProjectRoot:
    """One written project root, with the pieces a test wants to change."""

    path: Path

    @property
    def runtime(self) -> Path:
        return self.path / "runtime"

    @property
    def environment_text(self) -> str:
        return (self.path / "environment.yaml").read_text("utf-8")

    def write_code(self, source: str, *, module: str = "projectbmi") -> None:
        self.runtime.mkdir(parents=True, exist_ok=True)
        (self.runtime / f"{module}.py").write_text(source.lstrip("\n"), "utf-8")

    def write_vectors(self, document: str, *, name: str = "bmi") -> None:
        directory = self.path / "conformance"
        directory.mkdir(parents=True, exist_ok=True)
        (directory / f"{name}.yaml").write_text(document, "utf-8")

    def write_environment(self, **fields: str) -> str:
        """Write the environment, pinning the digest the runtime now has."""
        digest = artifact_digest(self.runtime)
        text = ENVIRONMENT.format(
            **{**ENVIRONMENT_DEFAULTS, **fields, "digest": digest}
        )
        (self.path / "environment.yaml").write_text(text, "utf-8")
        return digest

    def edit_environment(self, old: str, new: str) -> None:
        """Replace one written fragment, leaving every identity as it was."""
        text = self.environment_text
        assert old in text, old
        (self.path / "environment.yaml").write_text(text.replace(old, new, 1), "utf-8")

    def repin(self) -> str:
        """Pin the digest the runtime directory now hashes to."""
        digest = artifact_digest(self.runtime)
        text = self.environment_text
        marker = "    digest: sha256:"
        start = text.index(marker)
        end = text.index("\n", start)
        (self.path / "environment.yaml").write_text(
            text[:start] + f"    digest: {digest}" + text[end:], "utf-8"
        )
        return digest


@pytest.fixture(autouse=True)
def isolated_process_state():
    """Keep one case's activated artifacts and cached success out of the next.

    An artifact is identified by its content, so two roots holding the same
    code are one artifact and share one imported module. That is what
    pinning by content means in a run; between cases it would leak, so the
    process state an activation leaves behind is cleared around each one.
    """
    _clear_activated_artifacts()
    yield
    _clear_activated_artifacts()


def _clear_activated_artifacts() -> None:
    ACTIVATION_CACHE.clear()
    for name in [name for name in sys.modules if name.startswith("_yamaa_artifact_")]:
        del sys.modules[name]


@pytest.fixture(scope="session")
def repository() -> Repository:
    return Repository(_REPOSITORY_ROOT)


@pytest.fixture
def project(tmp_path: Path) -> ProjectRoot:
    """Return an empty project root a test fills in."""
    root = ProjectRoot(tmp_path / "project")
    root.path.mkdir(parents=True)
    return root


@pytest.fixture
def bmi_project(project: ProjectRoot, repository: Repository) -> ProjectRoot:
    """Return a written, correctly pinned copy of the BMI project root."""
    project.write_code(BMI_CODE)
    project.write_vectors(repository.vectors)
    project.write_environment()
    return project
