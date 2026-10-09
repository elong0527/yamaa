"""Independent metadata, normal binding and editable-provider boundary contracts."""

import sys
import tomllib
from importlib import metadata
from pathlib import Path

import pytest

from yamaa import _locked_functions as m


def lock(*records):
    text = "version = 1\n"
    for name, version, markers in records:
        text += f'\n[[package]]\nname = "{name}"\nversion = "{version}"\n'
        if markers is not None:
            text += (
                "resolution-markers = ["
                + ", ".join(repr(mark) for mark in markers)
                + "]\n"
            )
    return text.encode()


def verify(raw, calls, versions, providers, trace, environment=None):
    def observed_version(name):
        trace.append(("version", name))
        if name not in versions:
            raise metadata.PackageNotFoundError(name)
        return versions[name]

    def observed_providers():
        trace.append(("providers",))
        return providers

    return m.verify_versions(
        raw,
        calls,
        installed_version=observed_version,
        package_providers=observed_providers,
        environment=environment,
    )


def test_empty_selection_has_no_parsing_or_metadata_effects():
    trace = []
    assert verify(b"[malformed", [], {}, {}, trace) == ()
    assert trace == []


def test_stdlib_provider_mapping_precedes_exemption_and_yamaa_rechecks_each_activation():
    raw = lock(("yamaa", "0.2.0", None))
    trace = []
    for _ in range(2):
        assert (
            verify(raw, ["math.sqrt", "statistics.mean"], {"yamaa": "0.2.0"}, {}, trace)
            == ()
        )
    assert trace == [
        ("providers",),
        ("version", "yamaa"),
        ("providers",),
        ("version", "yamaa"),
    ]


def test_module_to_distribution_mapping_is_lossless_and_unrelated_packages_are_not_audited():
    raw = lock(
        ("yamaa", "0.2.0", None), ("project-bmi", "1.2.0", None), ("unused", "9", None)
    )
    trace = []
    assert (
        verify(
            raw,
            ["project_bmi.calculate", "project_bmi.other"],
            {"yamaa": "0.2.0", "project-bmi": "1.2"},
            {"project_bmi": ["Project_BMI"]},
            trace,
        )
        == ()
    )
    assert trace == [("providers",), ("version", "project-bmi"), ("version", "yamaa")]


def test_platform_forks_select_the_locked_version_independently_of_installed_version():
    raw = lock(
        ("yamaa", "0.2.0", None),
        ("project", "1", ["sys_platform == 'linux'"]),
        ("project", "2", ["sys_platform == 'win32'"]),
    )
    trace = []
    result = verify(
        raw,
        ["project.run"],
        {"yamaa": "0.2.0", "project": "2"},
        {"project": ["project"]},
        trace,
        {"sys_platform": "linux"},
    )
    assert result == (m.Finding("project", "version_mismatch", ("1",), "2"),)
    assert (
        verify(
            raw,
            ["project.run"],
            {"yamaa": "0.2.0", "project": "2"},
            {"project": ["project"]},
            [],
            {"sys_platform": "win32"},
        )
        == ()
    )


def test_all_independent_package_failures_are_reported_and_namespace_providers_all_checked():
    raw = lock(("yamaa", "0.2.0", None), ("a", "1", None), ("b", "2", None))
    trace = []
    result = verify(
        raw,
        ["shared.function", "unknown.call"],
        {"yamaa": "0.1.0", "a": "3"},
        {"shared": ["a", "b"]},
        trace,
    )
    assert result == (
        m.Finding("unknown", "distribution_not_identified"),
        m.Finding("a", "version_mismatch", ("1",), "3"),
        m.Finding("b", "package_not_installed", ("2",)),
        m.Finding("yamaa", "version_mismatch", ("0.2.0",), "0.1.0"),
    )
    assert trace == [
        ("providers",),
        ("version", "a"),
        ("version", "b"),
        ("version", "yamaa"),
    ]


def test_ambiguous_forks_are_not_chosen_by_the_installed_version():
    raw = lock(("yamaa", "0.2.0", None), ("project", "1", None), ("project", "2", None))
    trace = []
    assert verify(
        raw,
        ["project.run"],
        {"yamaa": "0.2.0", "project": "2"},
        {"project": ["project"]},
        trace,
    ) == (m.Finding("project", "ambiguous_lock_version", ("1", "2")),)
    assert ("version", "project") not in trace


@pytest.mark.parametrize(
    "raw",
    [
        b"\xff",
        b"version = true\npackage = []",
        b"version = 2\npackage = []",
        b"version = 1\npackage = {}",
        b"version = 1\n[[package]\n",
    ],
)
def test_invalid_lock_syntax_and_shape_precede_metadata(raw):
    trace = []
    with pytest.raises(m.InvalidLock):
        verify(raw, ["project.run"], {}, {}, trace)
    assert trace == []


def test_invalid_markers_and_missing_versions_do_not_become_installed_version_choices():
    for raw in [
        lock(("yamaa", "0.2.0", ["not a marker"])),
        b'version = 1\n[[package]]\nname = "yamaa"\n',
    ]:
        with pytest.raises(m.InvalidLock):
            verify(raw, ["math.sqrt"], {"yamaa": "0.2.0"}, {}, [])


def test_original_metadata_exception_and_interrupt_identity_are_preserved():
    for original in [RuntimeError("opaque metadata error"), KeyboardInterrupt("stop")]:

        def fail(name, original=original):
            raise original

        with pytest.raises(type(original)) as caught:
            m.verify_versions(
                lock(("yamaa", "0.2.0", None)), ["math.sqrt"], installed_version=fail
            )
        assert caught.value is original


def test_missing_installed_version_is_a_finding_without_guessing_or_stopping_other_checks():
    result = verify(
        lock(("yamaa", "0.2.0", None), ("project", "1", None)),
        ["project.run"],
        {"yamaa": "0.2.0", "project": None},
        {"project": ["project"]},
        [],
    )
    assert result == (m.Finding("project", "invalid_installed_version", ("1",)),)


def test_marker_and_identity_quotas_reject_before_installed_version_reads():
    for raw in [
        lock(("yamaa", "0.2.0", ['sys_platform == "linux"'] * 65)),
        lock(("yamaa", "1" * 2049, None)),
    ]:
        trace = []
        with pytest.raises(m.InvalidLock):
            verify(raw, ["math.sqrt"], {"yamaa": "0.2.0"}, {}, trace)
        assert not any(item[0] == "version" for item in trace)


def test_existing_repository_uv_lock_identifies_yamaa_and_a_called_distribution():
    raw = (Path(__file__).resolve().parents[2] / "uv.lock").read_bytes()
    versions = {
        record["name"]: record["version"]
        for record in tomllib.loads(raw.decode("utf-8"))["package"]
        if record["name"] in {"yamaa", "packaging"}
    }
    trace = []
    result = verify(
        raw,
        ["packaging.version.Version"],
        versions,
        {"packaging": ["packaging"]},
        trace,
    )
    assert result == ()
    assert trace == [("providers",), ("version", "packaging"), ("version", "yamaa")]


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


def test_resolution_bounds_precede_import(monkeypatch):
    def unreachable(_):
        raise AssertionError("metadata limit must precede import")

    monkeypatch.setattr(m.importlib, "import_module", unreachable)
    for call, parameters in [
        ("a" * 2049 + ".run", []),
        ("installed.run", ["x"] * 1025),
    ]:
        with pytest.raises(m.InvalidBinding):
            m.resolve_callable(call, parameters)


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
    trace = []

    def version(name):
        trace.append(name)
        return "0.2.0" if name == "yamaa" else "1.2.0"

    assert (
        m.verify_versions(
            lock(("yamaa", "0.2.0", None), ("clinical-programs", "1.2", None)),
            ["project_bmi.calculate"],
            installed_version=version,
        )
        == ()
    )
    assert trace == ["clinical-programs", "yamaa"]
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
    trace = []

    def version(name):
        trace.append(name)
        return {"yamaa": "0.2.0", "project-bmi": "1.2"}[name]

    assert (
        m.verify_versions(
            lock(("yamaa", "0.2.0", None), ("project-bmi", "1.2", None)),
            ["project_bmi.calculate"],
            installed_version=version,
        )
        == ()
    )
    assert trace == ["project-bmi", "yamaa"]
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


def test_distribution_with_stdlib_root_name_is_version_checked_before_exemption():
    trace = []
    result = verify(
        lock(("yamaa", "0.2.0", None), ("shadow-statistics", "1", None)),
        ["statistics.mean"],
        {"yamaa": "0.2.0", "shadow-statistics": "2"},
        {"statistics": ["shadow-statistics"]},
        trace,
    )
    assert result == (m.Finding("shadow-statistics", "version_mismatch", ("1",), "2"),)
    assert trace == [
        ("providers",),
        ("version", "shadow-statistics"),
        ("version", "yamaa"),
    ]


@pytest.mark.parametrize("lines", [("x" * 16385), ("import nonexistent_hook\n" * 65)])
def test_editable_path_bytes_and_line_counts_are_bounded_before_module_discovery(
    tmp_path, monkeypatch, lines
):
    dist, _ = editable_metadata(
        tmp_path, "Clinical-Programs", "project_bmi", lines=lines
    )
    monkeypatch.setattr(m.metadata, "packages_distributions", dict)
    monkeypatch.setattr(m.metadata, "distributions", lambda: iter([dist]))
    with pytest.raises(m.InvalidLock):
        m._installed_providers(("project_bmi",))
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
