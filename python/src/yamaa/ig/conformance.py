"""IG domain-model conformance checking (R028).

The check runs at study-document composition (R026-10): each dataset entry's
bound IG standard plus the specification's domain selects one versioned
template, and the dataset's ``output.columns`` are compared against it. A
specification validated without a study document is IG-agnostic and is never
checked here.
"""

from __future__ import annotations

from pathlib import Path
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field, JsonValue

from yamaa.specification._yaml import read_yaml_document
from yamaa.specification.loader import load_specification
from yamaa.specification.models import Specification

IgCondition = Literal[
    "ig_missing_required",
    "ig_missing_expected",
    "ig_unknown_variable",
    "ig_core_mismatch",
    "ig_no_template",
]
IgSeverity = Literal["error", "signal"]

# R024-5's closed family table: published standard name -> family. R028
# checks the sdtm and send families; anything else is out of scope.
_FAMILY_STANDARDS: dict[str, frozenset[str]] = {
    "sdtm": frozenset({"SDTMIG", "SDTMIG-AP", "SDTMIG-MD"}),
    "send": frozenset({"SENDIG", "SENDIG-AR", "SENDIG-DART", "SENDIG-GENETOX"}),
    "adam": frozenset({"ADaMIG", "ADaMIG-MD"}),
}
_CHECKED_FAMILIES = frozenset({"sdtm", "send"})

_CONDITION_REQUIREMENTS = {
    "ig_missing_required": "R028-6",
    "ig_missing_expected": "R028-7",
    "ig_unknown_variable": "R028-8",
    "ig_core_mismatch": "R028-9",
    "ig_no_template": "R028-5",
}


class IgDiagnostic(BaseModel):
    """One R028 finding: a validation failure or a reviewer signal."""

    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)

    condition: IgCondition
    severity: IgSeverity
    spec_paths: tuple[str, ...] = Field(min_length=1)
    requirement: str = Field(pattern=r"^R028-[0-9]+$")
    context: dict[str, JsonValue]
    message: str = Field(min_length=1)


class IgConformanceError(ValueError):
    """Raised when composition finds an error-severity R028 diagnostic."""

    def __init__(self, diagnostics: list[IgDiagnostic]) -> None:
        if not diagnostics:
            raise ValueError("IgConformanceError requires at least one diagnostic")
        self.diagnostics = tuple(diagnostics)
        conditions = ", ".join(diagnostic.condition for diagnostic in diagnostics)
        super().__init__(f"IG conformance failed: {conditions}")


class IgTemplateVariable(BaseModel):
    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)

    name: str
    core: Literal["Req", "Exp", "Perm"]
    label: str = ""


class IgTemplate(BaseModel):
    """One versioned IG domain model, keyed by family/name/version/domain."""

    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)

    family: str
    standard: str
    version: str
    domain: str
    variables: tuple[IgTemplateVariable, ...]

    @property
    def key(self) -> str:
        return f"{self.family}/{self.standard}/{self.version}/{self.domain}"


def family_for_standard(standard_name: str) -> str | None:
    """Return the R024-5 family for a published standard name, if any."""
    for family, names in _FAMILY_STANDARDS.items():
        if standard_name in names:
            return family
    return None


def load_template(
    templates_root: str | Path,
    family: str,
    standard_name: str,
    version: str,
    domain: str,
) -> IgTemplate | None:
    """Load one versioned template, or None when the repository has none."""
    path = Path(templates_root) / family / standard_name / version / f"{domain}.yaml"
    if not path.is_file():
        return None
    document = read_yaml_document(path)
    if not isinstance(document, dict):
        raise TypeError(f"IG template is not a mapping: {path}")
    header = document.get("template", {})
    variables = [
        IgTemplateVariable.model_validate(item)
        for item in document.get("variables", [])
    ]
    return IgTemplate(
        family=header.get("family", family),
        standard=header.get("standard", standard_name),
        version=str(header.get("version", version)),
        domain=header.get("domain", domain),
        variables=tuple(variables),
    )


def _edit_distance(left: str, right: str) -> int:
    """Levenshtein distance over two variable names."""
    previous = list(range(len(right) + 1))
    for left_index, left_char in enumerate(left, start=1):
        current = [left_index]
        for right_index, right_char in enumerate(right, start=1):
            insert = current[right_index - 1] + 1
            delete = previous[right_index] + 1
            replace = previous[right_index - 1] + (left_char != right_char)
            current.append(min(insert, delete, replace))
        previous = current
    return previous[len(right)]


def check_dataset_conformance(
    specification: Specification,
    *,
    standard_name: str,
    standard_version: str,
    templates_root: str | Path,
) -> list[IgDiagnostic]:
    """Compare one specification's output columns against its IG template.

    Returns every R028 finding, error-severity and signals alike; the caller
    decides what fails. A missing template yields one ``ig_no_template``
    signal and no checks, never a silent skip.
    """
    family = family_for_standard(standard_name)
    if family not in _CHECKED_FAMILIES:
        # R028-3: only sdtm/send are checked. An unknown family is R024-70's
        # failure, owned by composition, not by this check.
        return []
    template = load_template(
        templates_root, family, standard_name, standard_version, specification.domain
    )
    diagnostics: list[IgDiagnostic] = []
    if template is None:
        return [
            IgDiagnostic(
                condition="ig_no_template",
                severity="signal",
                spec_paths=("output.columns",),
                requirement=_CONDITION_REQUIREMENTS["ig_no_template"],
                context={
                    "family": family,
                    "standard": standard_name,
                    "version": standard_version,
                    "domain": specification.domain,
                },
                message=(
                    f"no IG template for {family}/{standard_name}/"
                    f"{standard_version}/{specification.domain}; "
                    "conformance was not checked"
                ),
            )
        ]

    template_names = {variable.name: variable for variable in template.variables}
    output_columns = list(specification.output.columns)
    declared_cores = {
        column.name: column.submission.core
        for column in specification.columns
        if column.submission is not None and column.submission.core is not None
    }

    def diagnostic(
        condition: IgCondition,
        spec_paths: tuple[str, ...],
        context: dict[str, JsonValue],
        message: str,
    ) -> IgDiagnostic:
        return IgDiagnostic(
            condition=condition,
            severity="error" if condition == "ig_missing_required" else "signal",
            spec_paths=spec_paths,
            requirement=_CONDITION_REQUIREMENTS[condition],
            context=context,
            message=message,
        )

    for variable in template.variables:
        if variable.name in output_columns:
            continue
        if variable.core == "Req":
            diagnostics.append(
                diagnostic(
                    "ig_missing_required",
                    ("output.columns",),
                    {"variable": variable.name, "template": template.key},
                    f"required IG variable {variable.name} is absent from "
                    f"output.columns ({template.key})",
                )
            )
        elif variable.core == "Exp":
            diagnostics.append(
                diagnostic(
                    "ig_missing_expected",
                    ("output.columns",),
                    {"variable": variable.name, "template": template.key},
                    f"expected IG variable {variable.name} is absent from "
                    f"output.columns ({template.key})",
                )
            )
        # R028-10: Perm variables are never flagged for omission.

    for column_name in output_columns:
        if column_name in template_names:
            declared = declared_cores.get(column_name)
            expected = template_names[column_name].core
            if declared is not None and declared != expected:
                diagnostics.append(
                    diagnostic(
                        "ig_core_mismatch",
                        (f"columns.{column_name}.submission.core",),
                        {
                            "variable": column_name,
                            "declared": declared,
                            "template": expected,
                        },
                        f"column {column_name} declares core {declared} but the "
                        f"IG template designates it {expected}",
                    )
                )
            continue
        context: dict[str, JsonValue] = {
            "variable": column_name,
            "template": template.key,
        }
        candidates = sorted(
            name for name in template_names if _edit_distance(column_name, name) <= 2
        )
        message = (
            f"output column {column_name} names no variable in the IG template "
            f"({template.key})"
        )
        if len(candidates) == 1:
            context["suggestion"] = candidates[0]
            message += f"; did you mean {candidates[0]}?"
        diagnostics.append(
            diagnostic(
                "ig_unknown_variable", (f"columns.{column_name}",), context, message
            )
        )

    return diagnostics


def check_study_document(
    define_path: str | Path,
    *,
    schema_root: str | Path,
    templates_root: str | Path | None = None,
) -> list[IgDiagnostic]:
    """Run R028 for every IG-bound dataset a study document declares.

    This is the R026-10 composition-time invocation: each ``datasets`` entry
    resolves its ``standard`` (defaulting to ``default_standard``), the entry
    is checked when the standard is an IG of the sdtm or send family, and any
    error-severity finding raises :class:`IgConformanceError`. The full
    define-document validation stays with R026; this helper reads only the
    binding fields the check needs.
    """
    define_file = Path(define_path)
    document = read_yaml_document(define_file)
    if not isinstance(document, dict):
        raise TypeError(f"study document is not a mapping: {define_file}")
    standards = {
        entry["id"]: entry
        for entry in document.get("standards", [])
        if isinstance(entry, dict) and "id" in entry
    }
    default_standard = document.get("default_standard")
    resolved_templates = (
        Path(templates_root)
        if templates_root is not None
        else Path(schema_root) / "templates" / "ig"
    )

    diagnostics: list[IgDiagnostic] = []
    for entry in document.get("datasets", []):
        if not isinstance(entry, dict):
            continue
        standard_id = entry.get("standard", default_standard)
        standard = standards.get(standard_id, {})
        if standard.get("type") != "IG":
            # R024-7 / R026-7 own non-IG bindings; R028 has nothing to check.
            continue
        name = standard.get("name", "")
        version = str(standard.get("version", ""))
        loaded = load_specification(define_file.parent / entry["spec"], schema_root)
        diagnostics.extend(
            check_dataset_conformance(
                loaded.specification,
                standard_name=name,
                standard_version=version,
                templates_root=resolved_templates,
            )
        )
    errors = [item for item in diagnostics if item.severity == "error"]
    if errors:
        raise IgConformanceError(errors)
    return diagnostics


__all__ = [
    "IgConformanceError",
    "IgDiagnostic",
    "IgTemplate",
    "IgTemplateVariable",
    "check_dataset_conformance",
    "check_study_document",
    "family_for_standard",
    "load_template",
]
