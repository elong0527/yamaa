"""Repository grammar document structure and rule-block editorial checks.

Typed parsers, exact AST shapes, dependencies and vocabulary boundaries are
asserted directly by the Rust contract-vector suite against the same YAML.
"""

import re
from pathlib import Path, PurePosixPath

import yaml
from tools.validation._repository_yaml import UniqueKeyLoader

GRAMMAR_DIR = PurePosixPath("yaml/grammar")


# Legacy owners of each grammar, used until a rule card file carries the
# grammar's marker. A card file renders a grammar as a ```text block directly
# after the line `<!-- grammar: NAME -->`.
GRAMMAR_CONTRACTS = {
    "predicate": "formulas",
    "numeric": "formulas",
    "string-template": "text",
    "aggregate": "formulas",
}
GRAMMAR_MARKER = re.compile(
    r"<!-- grammar: ([a-z-]+) -->\n+```text\n(.*?)\n```", re.DOTALL
)


GRAMMAR_DOCUMENT_KEYS = {
    "schema_version",
    "contract",
    "contract_version",
    "rule",
    "start",
    "productions",
    "imports",
    "vocabulary",
    "prohibited",
    "reserved",
    "cases",
}


GRAMMAR_REQUIRED_KEYS = (
    "schema_version",
    "contract",
    "contract_version",
    "rule",
    "start",
    "productions",
    "imports",
    "vocabulary",
    "cases",
)


GRAMMAR_PRODUCTION_KEYS = {"name", "definition", "prose"}


GRAMMAR_CASE_KEYS = {
    "id",
    "covers",
    "text",
    "parse",
    "condition",
    "identifiers",
    "shape",
}


GRAMMAR_COVERS = {
    "predicate": {
        "between",
        "call",
        "comparison",
        "escape",
        "grouping",
        "identifier",
        "in",
        "keyword-case",
        "like",
        "literal",
        "logic",
        "null-test",
        "precedence",
        "rejection",
        "reserved",
        "temporal",
    },
    "numeric": {
        "arithmetic",
        "call",
        "grouping",
        "identifier",
        "keyword-case",
        "literal",
        "null-literal",
        "precedence",
        "prohibited",
        "rejection",
        "unary",
        "vocabulary",
    },
    "string-template": {
        "escape",
        "identifier",
        "placeholder",
        "rejection",
        "text",
    },
    "aggregate": {
        "arithmetic",
        "call",
        "grouping",
        "identifier",
        "keyword-case",
        "literal",
        "null-literal",
        "precedence",
        "prohibited",
        "reduction",
        "rejection",
        "star",
        "unary",
        "vocabulary",
    },
}


def render_grammar_block(productions):
    """Render productions as the grammar block a rule carries.

    The name column is as wide as the longest production name. A
    continuation line that opens an alternative aligns its bar under the
    definition operator; every other continuation aligns under the
    definition itself.
    """
    width = max(len(production["name"]) for production in productions)
    lines = []
    for production in productions:
        definition = production["definition"].split("\n")
        lines.append(f"{production['name'].ljust(width)} := {definition[0]}")
        for continuation in definition[1:]:
            indent = width + (2 if continuation.startswith("|") else 4)
            lines.append(" " * indent + continuation)
    return "\n".join(lines)


def rule_grammar_block(text, contract=None):
    """Return the grammar block a rule renders for a contract.

    A marked block wins; a legacy contract's Grammar section is the fallback.
    """
    if contract is not None:
        for name, block in GRAMMAR_MARKER.findall(text):
            if name == contract:
                return block
    section = re.search(
        r"\n#{2,3} (?:Templates: )?Grammar\n(.*?)(?=\n#{2,3} |\Z)", text, re.DOTALL
    )
    if section is None:
        return None
    block = re.search(r"```text\n(.*?)\n```", section.group(1), re.DOTALL)
    return None if block is None else block.group(1)


def grammar_rule_id(root: Path, contract: str):
    """The rule file (relative to rules/, without .md) that renders a grammar."""
    owners = sorted(
        path.relative_to(root / "rules").with_suffix("").as_posix()
        for path in (root / "rules").rglob("*.md")
        if any(
            name == contract
            for name, _ in GRAMMAR_MARKER.findall(path.read_text(encoding="utf-8"))
        )
    )
    if len(owners) == 1:
        return owners[0]
    if not owners:
        return GRAMMAR_CONTRACTS[contract]
    return None


def grammar_symbol_references(definition):
    """Return the non-terminals an EBNF definition refers to."""
    without_terminals = re.sub(r'"[^"]*"', " ", definition)
    return set(re.findall(r"[A-Za-z_][A-Za-z0-9_]*", without_terminals))


def grammar_defined_symbols(document):
    """Return every symbol a grammar document defines or closes."""
    defined = set()
    productions = document.get("productions")
    if isinstance(productions, list):
        for production in productions:
            if isinstance(production, dict) and isinstance(production.get("name"), str):
                defined.add(production["name"])
    vocabulary = document.get("vocabulary")
    if isinstance(vocabulary, dict):
        defined.update(name for name in vocabulary if isinstance(name, str))
    for closed in ("prohibited", "reserved"):
        if closed in document:
            defined.add(closed)
    return defined


def validate_grammar_contract(root: Path, contract: str):
    """Check one authored grammar document against its owning rule."""
    label = str(GRAMMAR_DIR / f"{contract}.yaml")
    document_path = root / GRAMMAR_DIR / f"{contract}.yaml"
    if not document_path.exists():
        return [f"ERROR: {label}: missing machine-readable grammar"]

    try:
        with open(document_path, "r", encoding="utf-8") as handle:
            document = yaml.load(handle, Loader=UniqueKeyLoader)
    except Exception as exc:
        return [f"ERROR: {label}: {exc}"]

    if not isinstance(document, dict):
        return [f"ERROR: {label}: expected a mapping"]

    errors = []
    unknown = sorted(set(document) - GRAMMAR_DOCUMENT_KEYS)
    if unknown:
        errors.append(f"ERROR: {label}: unknown keys {unknown}")
    missing = [key for key in GRAMMAR_REQUIRED_KEYS if key not in document]
    if missing:
        errors.append(f"ERROR: {label}: missing keys {missing}")
        return errors
    if document["contract"] != contract:
        errors.append(
            f"ERROR: {label}: contract must be {contract!r}, got "
            f"{document['contract']!r}"
        )
    rule_id = grammar_rule_id(root, contract)
    if rule_id is None:
        errors.append(f"ERROR: {label}: more than one rule file renders it")
        return errors
    if document["rule"] != rule_id:
        errors.append(
            f"ERROR: {label}: rule must be {rule_id!r}, got {document['rule']!r}"
        )

    productions = document["productions"]
    if not isinstance(productions, list) or not productions:
        errors.append(f"ERROR: {label}: productions must be a non-empty list")
        return errors

    names = []
    for index, production in enumerate(productions):
        if not isinstance(production, dict):
            errors.append(f"ERROR: {label}: productions[{index}] must be a mapping")
            continue
        unknown = sorted(set(production) - GRAMMAR_PRODUCTION_KEYS)
        if unknown:
            errors.append(
                f"ERROR: {label}: productions[{index}] unknown keys {unknown}"
            )
        name = production.get("name")
        definition = production.get("definition")
        if not isinstance(name, str) or not name:
            errors.append(f"ERROR: {label}: productions[{index}].name must be a name")
            continue
        if name in names:
            errors.append(f"ERROR: {label}: {name}: duplicate production")
            continue
        names.append(name)
        if not isinstance(definition, str) or not definition:
            errors.append(
                f"ERROR: {label}: {name}: definition must be a non-empty string"
            )
        if "prose" in production and production["prose"] is not True:
            errors.append(f"ERROR: {label}: {name}: prose must be true or absent")
    if errors:
        return errors

    if document["start"] not in names:
        errors.append(
            f"ERROR: {label}: start {document['start']!r} names no production"
        )

    imports = document["imports"]
    if not isinstance(imports, dict):
        errors.append(f"ERROR: {label}: imports must be a mapping")
        imports = {}
    defined = grammar_defined_symbols(document) | set(imports)
    for production in productions:
        if production.get("prose") is True:
            continue
        undefined = sorted(
            grammar_symbol_references(production["definition"]) - defined
        )
        if undefined:
            errors.append(
                f"ERROR: {label}: {production['name']}: undefined symbols {undefined}"
            )

    for symbol, source in sorted(imports.items()):
        if source == "schema":
            continue
        if source not in GRAMMAR_CONTRACTS:
            errors.append(
                f"ERROR: {label}: {symbol} is imported from unknown contract {source!r}"
            )
            continue
        source_path = root / GRAMMAR_DIR / f"{source}.yaml"
        try:
            with open(source_path, "r", encoding="utf-8") as handle:
                source_document = yaml.load(handle, Loader=UniqueKeyLoader)
        except Exception:
            errors.append(
                f"ERROR: {label}: {symbol} is imported from {source!r}, "
                "which cannot be read"
            )
            continue
        if symbol not in grammar_defined_symbols(source_document):
            errors.append(
                f"ERROR: {label}: the {source!r} contract defines no "
                f"{symbol!r} to import"
            )

    errors.extend(grammar_vocabulary_errors(contract, document, label))

    rule_path = root / "rules" / f"{rule_id}.md"
    if not rule_path.is_file():
        errors.append(f"ERROR: {label}: rule {rule_id} has no file")
    else:
        block = rule_grammar_block(rule_path.read_text(encoding="utf-8"), contract)
        rendered = render_grammar_block(productions)
        if block is None:
            errors.append(
                f"ERROR: {rule_path.relative_to(root)}: no grammar block to "
                f"compare with {label}"
            )
        elif block != rendered:
            errors.append(
                f"ERROR: {rule_path.relative_to(root)}: the grammar block is "
                f"not the one {label} renders; replace it with:\n{rendered}"
            )

    cases = document["cases"]
    if not isinstance(cases, list) or not cases:
        errors.append(f"ERROR: {label}: cases must be a non-empty list")
        return errors

    seen = set()
    covered = set()
    for index, case in enumerate(cases):
        if not isinstance(case, dict):
            errors.append(f"ERROR: {label}: cases[{index}] must be a mapping")
            continue
        case_id = case.get("id")
        if not isinstance(case_id, str) or not case_id:
            errors.append(
                f"ERROR: {label}: cases[{index}].id must be a non-empty string"
            )
            continue
        if case_id in seen:
            errors.append(f"ERROR: {label}: {case_id}: duplicate case id")
            continue
        seen.add(case_id)
        if isinstance(case.get("covers"), list):
            covered.update(name for name in case["covers"] if isinstance(name, str))
        errors.extend(grammar_case_errors(contract, case, f"{label}: {case_id}"))

    missing_covers = sorted(GRAMMAR_COVERS[contract] - covered)
    if missing_covers:
        errors.append(f"ERROR: {label}: no case covers {missing_covers}")
    return errors


def validate_grammar_contracts(root: Path):
    """Check every closed grammar the language defines."""
    errors = []
    for contract in sorted(GRAMMAR_CONTRACTS):
        errors.extend(validate_grammar_contract(root, contract))
    return errors


def grammar_vocabulary_errors(contract, document, label):
    """Check declaration format; the Rust suite checks the declared language."""
    vocabulary = document.get("vocabulary")
    if not isinstance(vocabulary, dict):
        return [f"ERROR: {label}: vocabulary must be a mapping"]
    errors = []
    if contract == "numeric":
        functions = vocabulary.get("function")
        if not isinstance(functions, dict):
            return [f"ERROR: {label}: vocabulary.function is missing"]
        for name, arity in functions.items():
            if not isinstance(arity, dict) or set(arity) != {
                "min_arguments",
                "max_arguments",
            }:
                errors.append(
                    f"ERROR: {label}: function {name} must declare min_arguments and max_arguments"
                )
                continue
            lower, upper = arity["min_arguments"], arity["max_arguments"]
            if (
                type(lower) is not int
                or lower < 1
                or (upper is not None and (type(upper) is not int or upper < lower))
            ):
                errors.append(
                    f"ERROR: {label}: function {name} has invalid argument bounds"
                )
    elif contract == "aggregate":
        reducers = vocabulary.get("reducer")
        if not isinstance(reducers, dict):
            return [f"ERROR: {label}: vocabulary.reducer is missing"]
        for name, declaration in reducers.items():
            arguments = (
                declaration.get("argument") if isinstance(declaration, dict) else None
            )
            if (
                not isinstance(arguments, list)
                or not arguments
                or any(value not in {"expr", "star"} for value in arguments)
            ):
                errors.append(
                    f"ERROR: {label}: reducer {name} must declare its argument forms"
                )
    elif contract == "predicate":
        reserved = document.get("reserved")
        if not isinstance(reserved, list) or not all(
            isinstance(name, str) for name in reserved
        ):
            errors.append(f"ERROR: {label}: reserved must be a list of names")
        elif len(reserved) != len(set(reserved)):
            errors.append(f"ERROR: {label}: duplicate reserved name")
    elif vocabulary:
        errors.append(f"ERROR: {label}: vocabulary must be empty")
    return errors


def grammar_case_errors(contract, case, label):
    """Validate authored vector metadata without running a language parser."""
    errors = []
    unknown = sorted(set(case) - GRAMMAR_CASE_KEYS)
    if unknown:
        errors.append(f"ERROR: {label}: unknown keys {unknown}")
    covers = case.get("covers")
    if not isinstance(covers, list) or not covers:
        errors.append(f"ERROR: {label}: covers must be a non-empty list")
    else:
        outside = sorted(set(covers) - GRAMMAR_COVERS[contract])
        if outside:
            errors.append(f"ERROR: {label}: unknown covers {outside}")
    if not isinstance(case.get("text"), str):
        errors.append(f"ERROR: {label}: text must be a string")
    outcome = case.get("parse")
    if outcome not in {"accept", "reject"}:
        errors.append(f"ERROR: {label}: parse must be 'accept' or 'reject'")
        return errors
    if outcome == "reject":
        for key in ("identifiers", "shape"):
            if key in case:
                errors.append(f"ERROR: {label}: a rejected text records no {key}")
        if not isinstance(case.get("condition"), str) or not case["condition"]:
            errors.append(f"ERROR: {label}: condition must be a name")
        return errors
    if "condition" in case:
        errors.append(f"ERROR: {label}: an accepted text records no condition")
    identifiers = case.get("identifiers")
    if not isinstance(identifiers, list) or not all(
        isinstance(name, str) for name in identifiers
    ):
        errors.append(f"ERROR: {label}: identifiers must be a list of names")
    elif identifiers != sorted(set(identifiers)):
        errors.append(f"ERROR: {label}: identifiers must be unique sorted names")
    if not isinstance(case.get("shape"), str):
        errors.append(f"ERROR: {label}: shape must be a string")
    return errors
