"""Read a yamaa file in either language while both surfaces exist.

From the switch (#3030 P1-9) until P2-13, a benchmark is written in either
language, and the corpus moves to 1.0 one prefix at a time (P2-5 to P2-8).
A tool reads each file by its own header, as the engine does (D115): a root
that declares `yamaa` and no `schema_version` is a 1.x file
(`rust/crates/yamaa-core/src/language.rs`), and any other root is read under
the rules at the `yamaa-v0.5.0` tag (D75). P2-13 deletes the 0.5 half of each
tool with the 0.5 front end.
"""

import re

# A name in the closed expression grammar (yaml/grammar/expression.yaml): a
# letter or underscore, then letters, digits and underscores.
NAME = r"[A-Za-z_][A-Za-z0-9_]*"
REFERENCE = re.compile(rf"{NAME}(?:\.{NAME})?")
# A reserved word, matched without regard to case, is never a bare
# identifier; a qualified field may still use one after its qualifier.
RESERVED = frozenset(
    {"AND", "BETWEEN", "FALSE", "IN", "IS", "LIKE", "NOT", "NULL", "OR", "TRUE"}
)
TEXT = re.compile(r"'((?:[^']|'')*)'")
TEMPORAL = re.compile(r"(?:DATE|DATETIME)\s*'((?:[^']|'')*)'", re.IGNORECASE)
NUMBER = re.compile(r"[0-9]+(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?")


def declares_header(document):
    """Whether a parsed root is a 1.x file: it declares `yamaa` and no
    `schema_version`, as the engine routes a root (SCHEMA-2, D115)."""
    return (
        isinstance(document, dict)
        and "yamaa" in document
        and "schema_version" not in document
    )


def is_reference(text):
    """Whether grammar text is one bare or qualified name: a reference."""
    if not isinstance(text, str) or not REFERENCE.fullmatch(text):
        return False
    return "." in text or text.upper() not in RESERVED


def expression(value):
    """What the value of a 1.x expression field is (EXPR-5, #3030 D18).

    Returns `("reference", name)` for one bare or qualified name,
    `("literal", value)` for a constant (a YAML number, Boolean or null, or
    text that is one quoted string, temporal literal, number, `TRUE`, `FALSE`
    or `NULL`; quoted and temporal text give their unquoted text),
    `("formula", text)` for any other grammar text, and `("operation", value)`
    for a mapping, which holds one registered operation. Text in a 0.5
    expression field is a reference; this reads only 1.x files.
    """
    if isinstance(value, dict):
        return "operation", value
    if not isinstance(value, str):
        return "literal", value
    text = value.strip()
    if is_reference(text):
        return "reference", text
    quoted = TEXT.fullmatch(text) or TEMPORAL.fullmatch(text)
    if quoted:
        return "literal", quoted.group(1).replace("''", "'")
    if NUMBER.fullmatch(text):
        return "literal", text
    keyword = text.upper()
    if keyword == "NULL":
        return "literal", None
    if keyword in ("TRUE", "FALSE"):
        return "literal", keyword == "TRUE"
    return "formula", value
