"""The Rust lint (`yamaa.lint`) against the Python style tool (`yamaa.style`).

On any text, the Rust lint must report exactly the Python tool's findings,
less `literal_form`, which retires with `{literal: X}` and is not ported, and
its fix must write exactly the Python tool's fixed text. The Rust lint reads
the suppression comment as `# yamaa-lint:` (P-LINT-1), so its input and the
Python fix output respell `# yamaa-style:` on comment lines, the only lines a
suppression acts on. A text the Python tool cannot read the Rust lint refuses.
"""

import re
import tempfile
from pathlib import Path

import yaml

import yamaa
from yamaa.style import _field_orders, _findings
from yamaa.style._fix import fix_text

ORDERS = _field_orders()


def respell(text):
    parts = re.split(r"(\r\n|\r|\n)", text)
    return "".join(
        part.replace("yamaa-style:", "yamaa-lint:")
        if part.lstrip().startswith("#")
        else part
        for part in parts
    )


def python_rows(text):
    return [
        (item.line, item.column, item.name, item.spec_path, item.message)
        for item in _findings("case.yaml", text, ORDERS, None)
        if item.name != "literal_form"
    ]


def rust_rows(table):
    return [
        (row["line"], row["column"], row["finding"], row["spec_path"], row["message"])
        for row in table.iter_rows(named=True)
    ]


def assert_equal(text):
    """The Rust lint agrees with the Python tool on `text`, findings and fix."""
    with tempfile.TemporaryDirectory(prefix="yamaa-lint-equality-") as directory:
        path = Path(directory) / "case.yaml"
        path.write_bytes(respell(text).encode("utf-8"))
        try:
            expected = python_rows(text)
        except (yaml.YAMLError, UnicodeDecodeError):
            try:
                yamaa.lint(path)
            except yamaa.Error:
                return
            raise AssertionError("the Rust lint read a text the Python tool refused")
        assert rust_rows(yamaa.lint(path)) == expected
        fixed = fix_text(text, ORDERS, None)
        remaining = rust_rows(yamaa.lint(path, fix=True))
        assert path.read_bytes().decode("utf-8") == respell(fixed)
        assert remaining == python_rows(fixed)


def variants(text):
    """Benchmark files rearranged and padded line by line, so that each one
    exercises findings and fixes; a variant that is no longer YAML must be
    refused by both tools."""
    lines = text.split("\n")
    out = [
        # Blank lines removed: root_spacing and entry_spacing.
        "\n".join(line for line in lines if line.strip()) + "\n",
        # Blank lines doubled: blank_lines.
        text.replace("\n\n", "\n\n\n"),
        # A blank line after every key that opens a block: blank_lines.
        "\n".join(
            line + ("\n" if line.rstrip().endswith(":") else "") for line in lines
        ),
        # A leading blank line and no final line break.
        "\n" + text.rstrip("\n"),
    ]
    blocks, current = [], []
    for line in lines[: -1 if lines[-1] == "" else None]:
        if re.match(r"[A-Za-z_\"']", line) and current:
            blocks.append(current)
            current = []
        current.append(line)
    blocks.append(current)
    if len(blocks) > 2:
        # Root fields reversed after the first: field_order at the root.
        reordered = [blocks[0]] + blocks[:0:-1]
        out.append("\n".join("\n".join(block) for block in reordered) + "\n")
        out.append(
            "\n".join(line for block in reordered for line in block if line.strip())
            + "\n"
        )
    # The first two keys of each entry swapped: field_order inside entries.
    swapped = list(lines)
    for index in range(len(swapped) - 1):
        first = re.match(r"^(\s*)- ([A-Za-z_]+): (.+)$", swapped[index])
        second = first and re.match(
            rf"^{first.group(1)}  ([A-Za-z_]+): (.+)$", swapped[index + 1]
        )
        if second:
            swapped[index] = f"{first.group(1)}- {second.group(1)}: {second.group(2)}"
            swapped[index + 1] = f"{first.group(1)}  {first.group(2)}: {first.group(3)}"
    out.append("\n".join(swapped))
    padded, quoted, listed, mapped = [], [], [], []
    for line in lines:
        # Plain and quoted values past the line width: folding.
        plain = re.match(r"^(\s*(?:- )?[A-Za-z_]+: )([A-Za-z][^#\[\]{}]*)$", line)
        padded.append(
            line + " and more words" * 5 if plain and len(line) < 60 else line
        )
        quote = re.match(r'^(\s*(?:- )?[A-Za-z_]+: )"([^"]*)"$', line)
        quoted.append(
            f"{quote.group(1)}\"{quote.group(2)} AND X.Y = 'A B' OR X.Z IN ('1', "
            "'2') AND W.V > 0\""
            if quote
            else line
        )
        # Flow lists past the line width: wrapping after the bracket.
        flow = re.match(r"^(\s*(?:- )?[A-Za-z_]+: \[)(.*)\]$", line)
        columns = ", ".join(f"COLUMN{index:02d}" for index in range(10))
        listed.append(f"{flow.group(1)}{flow.group(2)}, {columns}]" if flow else line)
        # Flow mappings past the line width: block mappings.
        braces = re.match(r"^(\s*(?:- )?[A-Za-z_]+: \{)(.*)\}$", line)
        mapped.append(
            f"{braces.group(1)}{braces.group(2)}, padding_one: aaaaaaaaaa, "
            "padding_two: bbbbbbbbbb, padding_three: cccccccccc}"
            if braces and "literal" not in line
            else line
        )
    out += ["\n".join(padded), "\n".join(quoted), "\n".join(listed), "\n".join(mapped)]
    # CRLF and lone-CR endings.
    for variant in out[:2] + out[4:6]:
        out += [variant.replace("\n", "\r\n"), variant.replace("\n", "\r")]
    return [variant for variant in out if variant != text]
