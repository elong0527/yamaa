"""IG domain-model conformance checking (R028)."""

from yamaa.ig.conformance import (
    IgConformanceError,
    IgDiagnostic,
    IgTemplate,
    IgTemplateVariable,
    check_dataset_conformance,
    check_study_document,
    family_for_standard,
    load_template,
)

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
