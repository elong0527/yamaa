"""Installed metadata, normal binding and editable-provider boundary contracts.

The engine reads a function's uv lock and compares versions; its cases are in
rust/crates/yamaa-adapters/tests/uv_lock.rs. This host reports only the
installation, and a project run proves the two together.
"""

import json
import sys
from importlib import metadata
from pathlib import Path

import pytest
from packaging.markers import default_environment

import yamaa
from yamaa import _locked_functions as m


def test_standard_library_modules_are_named_by_the_interpreter():
    assert m.is_standard_library("math")
    assert m.is_standard_library("textwrap")
    assert not m.is_standard_library("project_bmi")
    assert not m.is_standard_library("yamaa")


def test_installed_versions_are_the_metadata_text_and_failures_keep_identity(
    monkeypatch,
):
    assert m.installed_version("yamaa") == metadata.version("yamaa")
    with pytest.raises(metadata.PackageNotFoundError):
        m.installed_version("yamaa-no-such-distribution")
    for original in [RuntimeError("opaque metadata error"), KeyboardInterrupt("stop")]:

        def fail(name, original=original):
            raise original

        monkeypatch.setattr(m.metadata, "version", fail)
        with pytest.raises(type(original)) as caught:
            m.installed_version("yamaa")
        assert caught.value is original


def test_marker_environment_is_the_one_packaging_evaluated_markers_in():
    assert m.marker_environment() == default_environment()


FUNCTION = """\
description: Text without its common leading white space.
params: [{name: text, type: str}]
returns: str
tests:
  - {id: normal, covers: [normal, boundary], args: {text: '  a'}, result: a}
  - {id: missing, covers: ['short-circuit-missing:text'], args: {text: null}, result: null}
"""
SPEC = """\
schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: {path: input.csv, types: {ID: int, TEXT: str}}}
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - {name: TEXT, type: str, derivation: {function: {name: dedent, args: {text: SRC.TEXT}}}}
output: {path: output.csv, columns: [ID, TEXT]}
"""


@pytest.fixture(params=["1.0", "0.5"])
def study(request, tmp_path):
    """A project whose specification calls `textwrap.dedent`, with its uv lock
    declared as a 1.0 project declares it, or as a 0.5 environment file does."""
    if request.param == "1.0":
        (tmp_path / "functions").mkdir()
        (tmp_path / "functions/dedent.yaml").write_text(
            'yamaa: "1.0"\ntype: function\nname: dedent\n'
            + FUNCTION
            + "implementations: {python: textwrap.dedent}\n"
        )
        (tmp_path / "project.yaml").write_text(
            'yamaa: "1.0"\ntype: project\nfunctions: [functions/dedent.yaml]\n'
            "environments:\n  python: {lock: uv.lock}\nadam:\n  specs: [spec.yaml]\n"
        )
    else:
        indented = "".join("    " + line + "\n" for line in FUNCTION.splitlines())
        (tmp_path / "environment.yaml").write_text(
            "schema_version: '1.0'\nlock: uv.lock\nfunctions:\n  dedent:\n"
            "    function: textwrap.dedent\n" + indented
        )
        (tmp_path / "project.yaml").write_text(
            "schema_version: '1.0'\nenvironments: {python: environment.yaml}\n"
            "adam: {specs: [spec.yaml]}\n"
        )
    (tmp_path / "spec.yaml").write_text(SPEC)
    (tmp_path / "input.csv").write_text("ID,TEXT\n1,a\n")
    return tmp_path


def lock(study, *records):
    """Write the study's uv lock of `(version, resolution-markers)` records
    for yamaa."""
    text = "version = 1\n"
    for version, markers in records:
        text += f'\n[[package]]\nname = "yamaa"\nversion = "{version}"\n'
        if markers is not None:
            text += f"resolution-markers = {json.dumps(markers)}\n"
    (study / "uv.lock").write_text(text)


def run(study):
    return yamaa.project(study / "project.yaml").spec("TEST").run()


def problems(result):
    """Each issue's condition and the facts of its context that a lock
    verification decides."""
    facts = ("package", "reason", "expected", "actual", "stage", "code")
    return [
        (
            issue["condition"],
            {
                key: value
                for key, value in json.loads(issue["context"]).items()
                if key in facts
            },
        )
        for issue in result.issues.to_dicts()
    ]


def mismatch(reason, expected, actual):
    return (
        "runtime_artifact_mismatch",
        {"package": "yamaa", "reason": reason, "expected": expected, "actual": actual},
    )


INSTALLED = metadata.version("yamaa")


def test_a_run_verifies_its_uv_lock_in_the_engine(study):
    lock(study, (INSTALLED, None))
    result = run(study)
    assert problems(result) == []
    assert result.output["TEXT"].to_list() == ["a"]
    lock(study, ("999999.0", None))
    result = run(study)
    assert result.output is None
    assert problems(result) == [mismatch("version_mismatch", ["999999.0"], INSTALLED)]


def test_resolution_markers_select_by_the_interpreter_s_marker_environment(study):
    here, elsewhere = f"sys_platform == '{sys.platform}'", "sys_platform == 'none'"
    lock(study, (INSTALLED, [here]), ("999999.0", [elsewhere]))
    assert problems(run(study)) == []
    lock(study, (INSTALLED, [elsewhere]), ("999999.0", [here]))
    assert problems(run(study)) == [
        mismatch("version_mismatch", ["999999.0"], INSTALLED)
    ]
    lock(study, (INSTALLED, ["python_full_version < '3'"]))
    assert problems(run(study)) == [mismatch("version_not_locked", [], None)]


@pytest.mark.parametrize(
    ("reply", "reason", "actual"),
    [
        (metadata.PackageNotFoundError("yamaa"), "package_not_installed", None),
        (None, "invalid_installed_version", None),
        ("not a version", "invalid_installed_version", "not a version"),
    ],
)
def test_the_engine_classifies_what_the_metadata_holds(
    study, monkeypatch, reply, reason, actual
):
    def observed(name):
        assert name == "yamaa"
        if isinstance(reply, Exception):
            raise reply
        return reply

    lock(study, ("1.0", None))
    monkeypatch.setattr(m, "installed_version", observed)
    assert problems(run(study)) == [mismatch(reason, ["1.0"], actual)]


def test_an_original_metadata_interrupt_keeps_its_identity(study, monkeypatch):
    original = KeyboardInterrupt("stop")

    def interrupted(name):
        raise original

    lock(study, (INSTALLED, None))
    monkeypatch.setattr(m, "installed_version", interrupted)
    with pytest.raises(KeyboardInterrupt) as caught:
        run(study)
    assert caught.value is original


def test_a_lock_the_engine_cannot_read_refuses_the_run_before_any_version(
    study, monkeypatch
):
    def unexpected(*arguments):
        raise AssertionError("observed after a refused lock")

    for name in ("installed_version", "marker_environment"):
        monkeypatch.setattr(m, name, unexpected)
    (study / "uv.lock").write_text(
        'version = 1\n[[package]]\nname = "yamaa"\nversion = "1"\n'
        'resolution-markers = ["not a marker"]\n'
    )
    result = run(study)
    assert result.output is None
    assert problems(result) == [
        ("engine_rejected", {"stage": "build", "code": "opaque_host"})
    ]


@pytest.fixture
def installed_callable(monkeypatch):
    from types import ModuleType

    module = ModuleType("yamaa_locked_test_module")
    monkeypatch.setitem(sys.modules, module.__name__, module)
    return module


def test_resolution_imports_normally_each_activation_without_running_code(
    installed_callable, monkeypatch
):
    trace = []

    def target(x, y=999):
        trace.append((x, y))
        return x + y

    installed_callable.target = target
    observed = []
    original_import = m.importlib.import_module

    def observe(name):
        observed.append(name)
        return original_import(name)

    monkeypatch.setattr(m.importlib, "import_module", observe)
    for _ in range(2):
        assert (
            m.resolve_callable("yamaa_locked_test_module.target", ["x", "y"]) is target
        )
    assert observed == ["yamaa_locked_test_module"] * 2
    assert trace == []
    assert target(x=1, y=100) == 101
    assert trace == [(1, 100)]


def test_keyword_only_and_reordered_names_resolve_without_parameter_renaming(
    installed_callable,
):
    def target(*, y, x):
        return x - y

    installed_callable.target = target
    result = m.resolve_callable("yamaa_locked_test_module.target", ["x", "y"])
    assert result(x=9, y=3) == 6
    with pytest.raises(m.InvalidBinding) as caught:
        m.resolve_callable("yamaa_locked_test_module.target", ["x", "renamed"])
    assert caught.value.missing == ("renamed",)
    assert caught.value.extra == ("y",)


@pytest.mark.parametrize(
    "target", [lambda x, /: x, lambda *args: args, lambda **kwargs: kwargs]
)
def test_closed_named_contract_rejects_positional_and_variadic_signatures(
    installed_callable, target
):
    installed_callable.target = target
    with pytest.raises(m.InvalidBinding) as caught:
        m.resolve_callable("yamaa_locked_test_module.target", ["x"])
    assert caught.value.unsupported


def test_writable_signature_and_wrapped_metadata_cannot_replace_actual_formals(
    installed_callable,
):
    import inspect

    def target(x):
        return x

    target.__signature__ = inspect.Signature(
        [inspect.Parameter("fake", inspect.Parameter.POSITIONAL_OR_KEYWORD)]
    )
    target.__wrapped__ = lambda fake: fake
    installed_callable.target = target
    assert m.resolve_callable("yamaa_locked_test_module.target", ["x"]) is target
    with pytest.raises(m.InvalidBinding):
        m.resolve_callable("yamaa_locked_test_module.target", ["fake"])


def test_bound_methods_and_callable_objects_inspect_actual_bound_formals(
    installed_callable,
):
    class Bound:
        def method(self, x):
            return x

        def __call__(self, x):
            return x + 1

    bound = Bound()
    for target in [bound.method, bound]:
        installed_callable.target = target
        assert m.resolve_callable("yamaa_locked_test_module.target", ["x"]) is target


def test_opaque_cached_callable_is_a_binding_failure_without_running_target(
    installed_callable,
):
    from functools import cache

    observed = []

    @cache
    def target(x):
        observed.append(x)
        return x

    installed_callable.target = target
    with pytest.raises(m.InvalidBinding) as caught:
        m.resolve_callable("yamaa_locked_test_module.target", ["x"])
    assert caught.value.call == "yamaa_locked_test_module.target"
    assert isinstance(caught.value.__cause__, (ValueError, TypeError))
    assert observed == []


@pytest.mark.parametrize("member", ["method", "__call__"])
def test_cached_bound_formals_are_a_binding_failure_without_running_target(
    installed_callable, member
):
    from functools import cache

    observed = []

    class Bound:
        @cache  # noqa: B019 -- exercise the installed cached-method boundary
        def method(self, x):
            observed.append(x)
            return x

        __call__ = method

    bound = Bound()
    installed_callable.target = bound.method if member == "method" else bound
    with pytest.raises(m.InvalidBinding) as caught:
        m.resolve_callable("yamaa_locked_test_module.target", ["x"])
    assert caught.value.call == "yamaa_locked_test_module.target"
    assert isinstance(caught.value.__cause__, ValueError)
    assert observed == []


@pytest.mark.parametrize("original", [ValueError("opaque"), TypeError("invalid")])
def test_unavailable_signature_retains_the_original_binding_failure_cause(
    installed_callable, monkeypatch, original
):
    installed_callable.target = lambda x: x

    def fail(_):
        raise original

    monkeypatch.setattr(m, "_concrete_signature", fail)
    with pytest.raises(m.InvalidBinding) as caught:
        m.resolve_callable("yamaa_locked_test_module.target", ["x"])
    assert caught.value.__cause__ is original


@pytest.mark.parametrize(
    "original",
    [RuntimeError("original introspection failure"), KeyboardInterrupt("stop")],
)
def test_other_introspection_errors_and_interrupts_retain_identity(
    installed_callable, monkeypatch, original
):
    installed_callable.target = lambda x: x

    def fail(_):
        raise original

    monkeypatch.setattr(m, "_concrete_signature", fail)
    with pytest.raises(type(original)) as caught:
        m.resolve_callable("yamaa_locked_test_module.target", ["x"])
    assert caught.value is original


@pytest.mark.parametrize(
    "original", [RuntimeError("original import failure"), KeyboardInterrupt("stop")]
)
def test_resolution_retains_original_import_exception_and_interrupt(
    monkeypatch, original
):
    def fail(_):
        raise original

    monkeypatch.setattr(m.importlib, "import_module", fail)
    with pytest.raises(type(original)) as caught:
        m.resolve_callable("installed_package.run", ["x"])
    assert caught.value is original


def test_missing_noncallable_and_duplicate_declarations_never_run_target(
    installed_callable,
):
    installed_callable.target = 3
    with pytest.raises(m.InvalidBinding):
        m.resolve_callable("yamaa_locked_test_module.target", ["x"])
    with pytest.raises(AttributeError):
        m.resolve_callable("yamaa_locked_test_module.absent", ["x"])
    installed_callable.target = lambda x: x
    with pytest.raises(m.InvalidBinding):
        m.resolve_callable("yamaa_locked_test_module.target", ["x", "x"])


def editable_metadata(tmp_path, distribution, root, *, lines=None):
    import json

    project = tmp_path / (distribution + "-source")
    source = project / "src"
    package = source / root
    package.mkdir(parents=True)
    (package / "__init__.py").write_text(
        "raise AssertionError('must not import before lock verification')\n"
    )
    installed = tmp_path / (distribution + "-installed")
    info = installed / (distribution.replace("-", "_") + "-1.2.dist-info")
    info.mkdir(parents=True)
    (info / "METADATA").write_text(
        f"Metadata-Version: 2.1\nName: {distribution}\nVersion: 1.2\n"
    )
    (info / "direct_url.json").write_text(
        json.dumps({"url": project.as_uri(), "dir_info": {"editable": True}})
    )
    pth = installed / (distribution + ".pth")
    pth.write_text(str(source) + "\n" if lines is None else lines)
    (info / "RECORD").write_text(
        f"{pth.name},,\n{info.name}/METADATA,,\n{info.name}/direct_url.json,,\n{info.name}/RECORD,,\n"
    )
    return metadata.Distribution.at(info), source


def test_editable_without_top_level_names_maps_actual_recorded_module_without_import(
    tmp_path, monkeypatch
):
    dist, _ = editable_metadata(tmp_path, "Clinical-Programs", "project_bmi")
    monkeypatch.setattr(m.metadata, "packages_distributions", dict)
    monkeypatch.setattr(m.metadata, "distributions", lambda: iter([dist]))
    assert m._installed_providers(("project_bmi",)) == {
        "project_bmi": ["Clinical-Programs"]
    }
    assert "project_bmi" not in sys.modules


def test_namespace_editable_providers_are_added_even_when_other_distribution_is_mapped(
    tmp_path, monkeypatch
):
    first, _ = editable_metadata(tmp_path, "Clinical-One", "shared")
    second, _ = editable_metadata(tmp_path, "Clinical-Two", "shared")
    monkeypatch.setattr(
        m.metadata, "packages_distributions", lambda: {"shared": ["Clinical-One"]}
    )
    monkeypatch.setattr(m.metadata, "distributions", lambda: iter([first, second]))
    assert m._installed_providers(("shared",)) == {
        "shared": ["Clinical-One", "Clinical-Two"]
    }


def test_recorded_editable_import_lines_never_execute_or_guess_distribution_name(
    tmp_path, monkeypatch
):
    dist, _ = editable_metadata(
        tmp_path,
        "Different-Distribution",
        "project_bmi",
        lines="import nonexistent_yamaa_metadata_hook\n",
    )
    monkeypatch.setattr(m.metadata, "packages_distributions", dict)
    monkeypatch.setattr(m.metadata, "distributions", lambda: iter([dist]))
    assert m._installed_providers(("project_bmi",)) == {}
    assert "nonexistent_yamaa_metadata_hook" not in sys.modules


def test_editable_paths_outside_the_registered_project_do_not_claim_modules(
    tmp_path, monkeypatch
):
    other = tmp_path / "unrelated"
    (other / "project_bmi").mkdir(parents=True)
    dist, _ = editable_metadata(
        tmp_path, "Clinical-Programs", "project_bmi", lines=str(other) + "\n"
    )
    monkeypatch.setattr(m.metadata, "packages_distributions", dict)
    monkeypatch.setattr(m.metadata, "distributions", lambda: iter([dist]))
    assert m._installed_providers(("project_bmi",)) == {}


def test_missing_editable_path_does_not_obstruct_other_called_package_metadata(
    tmp_path, monkeypatch
):
    dist, _ = editable_metadata(tmp_path, "Unrelated-Editable", "unused_programs")
    path = next(entry for entry in dist.files if entry.suffix == ".pth")
    Path(path.locate()).unlink()
    monkeypatch.setattr(
        m.metadata, "packages_distributions", lambda: {"project_bmi": ["project-bmi"]}
    )
    monkeypatch.setattr(m.metadata, "distributions", lambda: iter([dist]))
    assert m._installed_providers(("project_bmi",)) == {"project_bmi": ["project-bmi"]}
    assert "unused_programs" not in sys.modules
    assert "project_bmi" not in sys.modules


@pytest.mark.parametrize(
    "original",
    [
        PermissionError("path denied"),
        OSError("path unreadable"),
        KeyboardInterrupt("stop"),
    ],
)
def test_editable_path_read_errors_and_interrupts_retain_original_identity(
    tmp_path, monkeypatch, original
):
    dist, _ = editable_metadata(tmp_path, "Clinical-Programs", "project_bmi")
    monkeypatch.setattr(m.metadata, "packages_distributions", dict)
    monkeypatch.setattr(m.metadata, "distributions", lambda: iter([dist]))
    real_open = Path.open

    def open_path(path, *args, **kwargs):
        if path.suffix == ".pth":
            raise original
        return real_open(path, *args, **kwargs)

    monkeypatch.setattr(Path, "open", open_path)
    with pytest.raises(type(original)) as caught:
        m._installed_providers(("project_bmi",))
    assert caught.value is original


def test_many_recorded_import_lines_are_not_executed_before_a_real_editable_path(
    tmp_path, monkeypatch
):
    dist, source = editable_metadata(tmp_path, "Clinical-Programs", "project_bmi")
    pth = next(entry for entry in dist.files if entry.suffix == ".pth")
    Path(pth.locate()).write_text(
        "import nonexistent_hook\n" * 1000 + str(source) + "\n"
    )
    monkeypatch.setattr(m.metadata, "packages_distributions", dict)
    monkeypatch.setattr(m.metadata, "distributions", lambda: iter([dist]))
    assert m._installed_providers(("project_bmi",)) == {
        "project_bmi": ["Clinical-Programs"]
    }
    assert "nonexistent_hook" not in sys.modules
    assert "project_bmi" not in sys.modules


@pytest.mark.parametrize(
    "original",
    [
        OSError("original installed metadata failure"),
        KeyboardInterrupt("metadata interrupted"),
    ],
)
def test_editable_metadata_original_errors_and_interrupts_are_preserved(
    monkeypatch, original
):
    class BrokenMetadata:
        def read_text(self, name):
            raise original

    monkeypatch.setattr(m.metadata, "packages_distributions", dict)
    monkeypatch.setattr(m.metadata, "distributions", lambda: iter([BrokenMetadata()]))
    with pytest.raises(type(original)) as caught:
        m._installed_providers(("project_bmi",))
    assert caught.value is original


def test_editable_metadata_invalid_utf8_is_invalid_metadata(monkeypatch):
    original = UnicodeDecodeError("utf-8", b"\xff", 0, 1, "invalid start byte")

    class UndecodableMetadata:
        def read_text(self, name):
            raise original

    monkeypatch.setattr(m.metadata, "packages_distributions", dict)
    monkeypatch.setattr(
        m.metadata, "distributions", lambda: iter([UndecodableMetadata()])
    )
    with pytest.raises(m.InvalidMetadata, match="invalid editable metadata") as caught:
        m._installed_providers(("project_bmi",))
    assert caught.value.__cause__ is original
