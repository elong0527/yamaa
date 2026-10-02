"""Study composition, Define-XML generation, and bound terminology checks."""

from yamaa.specification.terminology import load_study_document
from yamaa.submission.composition import ComposedStudy, compose_study
from yamaa.submission.define_xml import render_define_xml, validate_define_xml
from yamaa.submission.generation import generate_study_document

__all__ = [
    "ComposedStudy",
    "compose_study",
    "generate_study_document",
    "load_study_document",
    "render_define_xml",
    "validate_define_xml",
]
