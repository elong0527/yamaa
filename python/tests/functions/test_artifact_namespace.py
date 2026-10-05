"""Distinct artifact references must never share a module identity.

The artifact namespace is derived from its reference (REQ-0666), so the
escaping must be injective: references such as ``my-artifact`` and
``my_artifact`` previously both became ``_yamaa_artifact_my_artifact``,
and the second artifact silently executed the first artifact's code.
"""

from __future__ import annotations

import sys
from pathlib import Path

from yamaa.functions.artifact import LoadedArtifact


def _write_module(root: Path, body: str) -> Path:
    root.mkdir(parents=True, exist_ok=True)
    module = root / "mod.py"
    module.write_text(body, encoding="utf-8")
    return root


def test_distinct_references_get_distinct_namespaces(tmp_path):
    first = _write_module(tmp_path / "one", 'def who(): return "from-artifact-one"\n')
    second = _write_module(tmp_path / "two", 'def who(): return "from-artifact-two"\n')
    artifact_one = LoadedArtifact(reference="my-artifact", root=first)
    artifact_two = LoadedArtifact(reference="my_artifact", root=second)
    assert artifact_one.namespace != artifact_two.namespace
    assert artifact_one.load("mod.who")() == "from-artifact-one"
    assert artifact_two.load("mod.who")() == "from-artifact-two"


def test_namespace_escaping_is_injective(tmp_path):
    """Literal escape-looking text must not collide with an encoded char."""
    roots = {
        reference: _write_module(tmp_path / f"root-{index}", "VALUE = 1\n")
        for index, reference in enumerate(["a-b", "a_b", "a_x2d_b", "a b"])
    }
    namespaces = {
        LoadedArtifact(reference=reference, root=root).namespace
        for reference, root in roots.items()
    }
    assert len(namespaces) == 4
    for namespace in namespaces:
        assert namespace.startswith("_yamaa_artifact_")


def test_one_artifact_does_not_leak_modules_into_another(tmp_path):
    """Loading from the second artifact must not reuse the first's modules."""
    first = _write_module(tmp_path / "one", 'def who(): return "one"\n')
    second = _write_module(tmp_path / "two", 'def who(): return "two"\n')
    one = LoadedArtifact(reference="dash-artifact", root=first)
    two = LoadedArtifact(reference="dash_artifact", root=second)
    assert one.load("mod.who")() == "one"
    assert two.load("mod.who")() == "two"
    module_names = [
        name
        for name in sys.modules
        if name.startswith("_yamaa_artifact_")
    ]
    assert len({name.split(".")[0] for name in module_names}) == 2
