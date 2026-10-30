"""Check the YAML examples on the site against their file types' JSON Schemas.

The check reads the YAML that readers copy: the site pages (`README.md`,
`rules/README.md` and `docs/articles/*.md`), the package READMEs, and the
placeholders and prefilled values of the issue forms. It reads each fenced
`yaml` block, and the whole text of a form field that renders as YAML, as the
repository checks read YAML.

- A block whose root declares `yamaa:` is a whole 1.0 file. It must parse, its
  `type:` must name a file type, and it must validate against that type's JSON
  Schema in JSON_SCHEMAS under the Draft 2020-12 validator.
- A block whose root has no `yamaa:`, such as the style guide's excerpts or an
  elided `...`, is not a whole file, so no schema applies to it.
- A block that is deliberately partial starts with the line `# fragment`, and
  the check skips it.

CONTRIBUTING.md's "Documentation" section states the convention for authors.

No page the check reads holds a 0.5 example since #3703 rewrote the schema
introduction, so REFUSE_SCHEMA_VERSION is on: a block that declares
`schema_version`, the 0.5 header, fails, even one that starts with
`# fragment`. The release notes (`docs/releases/`) keep their 0.5 examples,
and this check does not read them.
"""

import argparse
import json
import re
import sys
from pathlib import Path

from jsonschema import Draft202012Validator

import yaml
from tools.paths import ROOT
from tools.validation._repository_yaml import UniqueKeyLoader

# The pages, as patterns under the repository root, and the issue forms.
PAGES = (
    "README.md",
    "rules/README.md",
    "docs/articles/*.md",
    "python/README.md",
    "R/yamaa/README.md",
)
FORMS = ".github/ISSUE_TEMPLATE/*.yml"
# Where the JSON Schemas are committed.
JSON_SCHEMAS = Path("yaml/json")
SCHEMA_SUFFIX = ".schema.json"
FRAGMENT = "# fragment"
# On since #3703 took the last 0.5 example, in the schema introduction, off
# the pages.
REFUSE_SCHEMA_VERSION = True
# A Markdown code fence and its info string, whose first word is the language.
FENCE = re.compile(r"(?P<indent> *)(?P<fence>`{3,}|~{3,})(?P<info>.*)")
YAML_LANGUAGES = ("yaml", "yml")
# How a block that does not parse shows that its root declares `yamaa:`; YAML
# admits an indented root mapping.
HEADER_LINE = re.compile(r"^yamaa *:", re.MULTILINE)


def declares_header(text):
    """An unparsed block's header at its root mapping's indentation.

    Comments and document markers can precede an indented root. A nested
    `yamaa` key in an excerpt is not that root's header.
    """
    lines = [
        line
        for line in text.splitlines()
        if line.strip()
        and not line.lstrip().startswith("#")
        and line.strip() not in ("---", "...")
    ]
    if not lines:
        return False
    indent = len(lines[0]) - len(lines[0].lstrip(" "))
    return any(HEADER_LINE.match(line[indent:]) for line in lines)


def fenced_blocks(text):
    """Each fenced YAML block of Markdown text: the line its text starts on,
    counted from 1, and its text without the fence's indentation."""
    lines = text.splitlines()
    index = 0
    while index < len(lines):
        opening = FENCE.fullmatch(lines[index])
        index += 1
        if opening is None or (opening["fence"][0] == "`" and "`" in opening["info"]):
            continue
        start, indent, fence = index + 1, len(opening["indent"]), opening["fence"]
        body = []
        while index < len(lines):
            line = lines[index]
            index += 1
            depth, closing = len(line) - len(line.lstrip(" ")), line.strip()
            # A fence indented more than three spaces past the opening one is
            # the block's text.
            if (
                depth <= indent + 3
                and closing.startswith(fence)
                and closing == fence[0] * len(closing)
            ):
                break
            body.append(line[min(indent, depth) :])
        words = opening["info"].split()
        if words and words[0].lower() in YAML_LANGUAGES:
            yield start, "".join(f"{line}\n" for line in body)


def member(node, key):
    """The value node of `key` in a YAML mapping node, or None."""
    if isinstance(node, yaml.MappingNode):
        for name, value in node.value:
            if name.value == key:
                return value
    return None


def form_blocks(text):
    """Each YAML block of an issue form: the line its text starts on and its
    text.

    A field's placeholder or prefilled value is Markdown, unless the field
    renders its text as YAML, which makes the whole text one block. A literal
    block scalar's text starts on the line after its indicator."""
    body = member(yaml.compose(text, Loader=yaml.SafeLoader), "body")
    for item in body.value if isinstance(body, yaml.SequenceNode) else ():
        attributes = member(item, "attributes")
        render = member(attributes, "render")
        whole = isinstance(render, yaml.ScalarNode) and render.value == "yaml"
        for key in ("placeholder", "value"):
            node = member(attributes, key)
            if not isinstance(node, yaml.ScalarNode):
                continue
            first = node.start_mark.line + (2 if node.style in ("|", ">") else 1)
            if whole:
                yield first, node.value
            else:
                for line, block in fenced_blocks(node.value):
                    yield first + line - 1, block


def blocks(root):
    """Each YAML block the check reads: its file, relative to `root`, the line
    its text starts on, and its text."""
    for pattern in PAGES:
        for path in sorted(root.glob(pattern)):
            name = path.relative_to(root).as_posix()
            for line, text in fenced_blocks(path.read_text(encoding="utf-8")):
                yield name, line, text
    for path in sorted(root.glob(FORMS)):
        name = path.relative_to(root).as_posix()
        for line, text in form_blocks(path.read_text(encoding="utf-8")):
            yield name, line, text


def check(root, refuse_schema_version=REFUSE_SCHEMA_VERSION):
    """The messages for the blocks that fail, and how many 1.0 files were
    validated."""
    validators = {
        path.name.removesuffix(SCHEMA_SUFFIX): Draft202012Validator(
            json.loads(path.read_text(encoding="utf-8"))
        )
        for path in sorted((root / JSON_SCHEMAS).glob(f"*{SCHEMA_SUFFIX}"))
    }
    errors, validated = [], 0
    for name, line, text in blocks(root):
        fragment = text.partition("\n")[0].rstrip() == FRAGMENT
        try:
            document = yaml.load(text, Loader=UniqueKeyLoader)
        except yaml.YAMLError as error:
            if not fragment and declares_header(text):
                mark = getattr(error, "problem_mark", None)
                problem = getattr(error, "problem", None) or str(error)
                errors.append(
                    f"{name}:{line + (mark.line if mark else 0)}: the block does "
                    f"not parse as YAML: {problem}"
                )
            continue
        if not isinstance(document, dict):
            continue
        if refuse_schema_version and "schema_version" in document:
            errors.append(
                f"{name}:{line}: the block declares `schema_version`, the 0.5 "
                'header; write it as a 1.0 file, with `yamaa: "1.0"` and `type:`'
            )
            continue
        if fragment or "yamaa" not in document:
            continue
        if "type" not in document:
            errors.append(
                f"{name}:{line}: the block declares `yamaa:` without `type:`; "
                f"add its file type, or start a partial block with `{FRAGMENT}`"
            )
            continue
        file_type = document["type"]
        validator = validators.get(file_type) if isinstance(file_type, str) else None
        if validator is None:
            errors.append(
                f"{name}:{line}: `type: {file_type}` names no file type with a "
                f"JSON Schema in {JSON_SCHEMAS.as_posix()}/"
            )
            continue
        validated += 1
        for error in validator.iter_errors(document):
            errors.append(
                f"{name}:{line}: the {file_type} file at {error.json_path}: "
                f"{error.message}"
            )
    return errors, validated


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    errors, validated = check(parser.parse_args(argv).root)
    for error in errors:
        print(f"ERROR: {error}")
    if errors:
        return 1
    print(
        "PASS: whole 1.0 files on the site pages, in the package READMEs and in "
        f"the issue forms validate against their JSON Schemas ({validated} checked)."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
