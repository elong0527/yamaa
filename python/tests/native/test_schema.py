"""yamaa.schema(): the JSON Schema of a file type (#3030 section 2.12, D149).

The engine embeds the JSON Schema that the repository commits for each file
type in yaml/json/ and returns it as canonical JSON text: the committed
document on one line, members in key order. These controls compare the
installed host with the committed files, so they need the compiled native
extension and the checkout.
"""

import inspect
import json
from pathlib import Path

import pytest

pytest.importorskip(
    "yamaa._native", reason="the compiled native extension is not installed"
)

import yamaa

COMMITTED = Path(__file__).resolve().parents[3] / "yaml" / "json"
FILE_TYPES = ["spec", "project", "codelists", "function"]


def canonical(file_type):
    """The committed JSON Schema of `file_type` as canonical JSON text."""
    document = json.loads((COMMITTED / f"{file_type}.schema.json").read_bytes())
    return json.dumps(document, sort_keys=True, separators=(",", ":"))


def test_schema_takes_one_file_type_and_is_exported():
    assert "schema" in yamaa.__all__
    assert list(inspect.signature(yamaa.schema).parameters) == ["file_type"]
    assert sorted(path.name for path in COMMITTED.iterdir()) == sorted(
        f"{file_type}.schema.json" for file_type in FILE_TYPES
    )


@pytest.mark.parametrize("file_type", FILE_TYPES)
def test_each_file_type_returns_its_committed_json_schema(file_type):
    text = yamaa.schema(file_type)
    assert isinstance(text, str)
    assert text == canonical(file_type)
    assert text.isascii() and "\n" not in text
    assert json.loads(text)["title"] == f"yamaa 1.0 {file_type} file"


@pytest.mark.parametrize("file_type", ["specs", "Spec", " spec", "", "environment"])
def test_any_other_text_raises_the_refusal_with_its_issue(file_type):
    with pytest.raises(yamaa.Error) as raised:
        yamaa.schema(file_type)
    error = raised.value
    assert str(error) == (
        f"cannot give the schema of {file_type}: a {file_type} file is given where "
        "a spec, project, codelists, function file is expected "
        "[unexpected_file_type, SCHEMA-2]"
    )
    assert error.paths == []
    [issue] = error.issues.to_pylist()
    assert issue["severity"] == "error"
    assert issue["phase"] == "validation"
    assert issue["condition"] == "unexpected_file_type"
    assert issue["rule"] == "SCHEMA-2"
    assert issue["spec_paths"] == []
    assert issue["file"] is None and issue["line"] is None
    assert json.loads(issue["context"]) == {"expected": FILE_TYPES, "type": file_type}


@pytest.mark.parametrize("value", [None, 1, b"spec", ["spec"]])
def test_an_argument_that_is_not_text_is_a_package_error(value):
    with pytest.raises(yamaa.Error):
        yamaa.schema(value)
