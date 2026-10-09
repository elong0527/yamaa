"""Host metadata checks for the new locked environment activation port.

The shared engine supplies already admitted callable names. This module reads
packaging-tool metadata and installed versions; it does not import project code,
construct a semantic plan, install anything or cache activation success.
"""

from __future__ import annotations

import importlib
import inspect
import json
import sys
import tomllib
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass
from importlib import metadata
from pathlib import Path
from types import FunctionType, MethodType
from urllib.parse import urlsplit
from urllib.request import url2pathname

from packaging.markers import InvalidMarker, Marker
from packaging.utils import InvalidName, canonicalize_name
from packaging.version import InvalidVersion, Version

_MAX_BYTES = 16_777_216
_MAX_RECORDS = 65_536
_MAX_CALLS = 1_024
_MAX_MARKER_BYTES = 2_048
_MAX_MARKERS = 64
_MAX_IDENTITY_BYTES = 2_048


def _installed_providers(roots: Sequence[str]) -> Mapping[str, list[str]]:
    """Supplement installed import names with non-executable editable metadata.

    PEP 660 path-based editables can omit top_level.txt. Their installed direct
    URL plus recorded .pth paths identify the owning distribution mechanically.
    Never evaluate .pth import lines or import a package to infer its owner.
    """
    mapping = {
        root: list(names)
        for root, names in metadata.packages_distributions().items()
        if root in roots
    }
    remaining_files = _MAX_RECORDS
    remaining_metadata = _MAX_BYTES
    remaining_probes = 262_144
    for index, distribution in enumerate(metadata.distributions()):
        if index >= _MAX_RECORDS:
            raise InvalidLock("installed distribution count limit")
        raw = distribution.read_text("direct_url.json")
        if raw is None:
            continue
        if len(raw.encode("utf-8")) > 16_384:
            raise InvalidLock("editable metadata byte limit")
        remaining_metadata -= len(raw.encode("utf-8"))
        if remaining_metadata < 0:
            raise InvalidLock("installed metadata cumulative byte limit")
        try:
            direct = json.loads(raw)
        except (ValueError, RecursionError) as error:
            raise InvalidLock("invalid editable metadata") from error
        if (
            not isinstance(direct, dict)
            or not isinstance(direct.get("dir_info"), dict)
            or direct["dir_info"].get("editable") is not True
        ):
            continue
        url = direct.get("url")
        if not isinstance(url, str) or len(url.encode("utf-8")) > _MAX_IDENTITY_BYTES:
            raise InvalidLock("editable source identity limit")
        parsed = urlsplit(url)
        if (
            parsed.scheme != "file"
            or parsed.netloc not in ("", "localhost")
            or parsed.query
            or parsed.fragment
        ):
            raise InvalidLock("editable source is not a local file URL")
        project = Path(url2pathname(parsed.path)).resolve()
        name = distribution.metadata.get("Name")
        if not isinstance(name, str) or len(name.encode("utf-8")) > _MAX_IDENTITY_BYTES:
            raise InvalidLock("editable distribution identity limit")
        for entry in distribution.files or ():
            remaining_files -= 1
            if remaining_files < 0:
                raise InvalidLock("editable file metadata limit")
            if entry.suffix != ".pth":
                continue
            installed = Path(entry.locate())
            with installed.open("rb") as stream:
                held = stream.read(16_385)
            if len(held) > 16_384:
                raise InvalidLock("editable path metadata byte limit")
            remaining_metadata -= len(held)
            if remaining_metadata < 0:
                raise InvalidLock("installed metadata cumulative byte limit")
            try:
                lines = held.decode("utf-8").splitlines()
            except UnicodeDecodeError as error:
                raise InvalidLock("invalid editable path metadata") from error
            if len(lines) > 64:
                raise InvalidLock("editable path count limit")
            for line in lines:
                line = line.strip()
                if not line or line.startswith(("#", "import ", "import\t")):
                    continue
                if len(line.encode("utf-8")) > _MAX_IDENTITY_BYTES:
                    raise InvalidLock("editable path identity limit")
                path = (installed.parent / line).resolve()
                if not path.is_relative_to(project):
                    continue
                for root in roots:
                    remaining_probes -= 1
                    if remaining_probes < 0:
                        raise InvalidLock("editable module discovery work limit")
                    if (path / root).is_dir() or (path / (root + ".py")).is_file():
                        names = mapping.setdefault(root, [])
                        if name not in names:
                            names.append(name)
    return mapping


@dataclass(frozen=True, slots=True)
class Finding:
    package: str
    reason: str
    expected: tuple[str, ...] = ()
    actual: str | None = None


class InvalidLock(ValueError):
    """Held bytes cannot be interpreted as a bounded uv packaging lock."""


def _read(raw: bytes) -> dict:
    if len(raw) > _MAX_BYTES:
        raise InvalidLock("lock byte limit")
    try:
        document = tomllib.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, tomllib.TOMLDecodeError) as error:
        raise InvalidLock("invalid uv lock syntax") from error
    records = document.get("package")
    if type(document.get("version")) is not int or document["version"] != 1:
        raise InvalidLock("unsupported uv lock format")
    if not isinstance(records, list) or len(records) > _MAX_RECORDS:
        raise InvalidLock("invalid uv package array")
    return document


def _selected(record: dict, environment: Mapping[str, str] | None) -> bool:
    markers = record.get("resolution-markers")
    if markers is None:
        return True
    if not isinstance(markers, list) or not markers or len(markers) > _MAX_MARKERS:
        raise InvalidLock("invalid resolution markers")
    results = []
    for text in markers:
        if not isinstance(text, str) or len(text.encode("utf-8")) > _MAX_MARKER_BYTES:
            raise InvalidLock("resolution marker byte limit")
        try:
            results.append(
                Marker(text).evaluate(environment=environment, context="lock_file")
            )
        except (InvalidMarker, KeyError, ValueError, RecursionError) as error:
            raise InvalidLock("invalid resolution marker") from error
    return any(results)


def verify_versions(
    raw: bytes,
    calls: Sequence[str],
    *,
    installed_version: Callable[[str], str] | None = None,
    package_providers: Callable[[], Mapping[str, list[str]]] | None = None,
    environment: Mapping[str, str] | None = None,
) -> tuple[Finding, ...]:
    """Verify yamaa and every called distribution before callable import.

    No called functions means no lock parsing or host metadata observations.
    Namespace providers are all checked; unrelated packages are not audited.
    Ambiguous applicable versions are rejected rather than chosen by what happens
    to be installed. Version comparison uses the packaging tool's PEP 440 rules.
    """
    if not calls:
        return ()
    if len(calls) > _MAX_CALLS:
        raise InvalidLock("called-function limit")
    document = _read(raw)
    version = metadata.version if installed_version is None else installed_version
    needed = {"yamaa"}
    findings = []
    roots = tuple(dict.fromkeys(call.split(".", 1)[0] for call in calls))
    if roots:
        mapping = (
            _installed_providers(roots)
            if package_providers is None
            else package_providers()
        )
        for root in roots:
            distributions = mapping.get(root)
            if not distributions:
                if root in sys.stdlib_module_names:
                    continue
                findings.append(Finding(root, "distribution_not_identified"))
                continue
            for distribution in distributions:
                if (
                    not isinstance(distribution, str)
                    or len(distribution.encode("utf-8")) > _MAX_IDENTITY_BYTES
                ):
                    raise InvalidLock("installed distribution identity limit")
                try:
                    needed.add(canonicalize_name(distribution, validate=True))
                    if len(needed) > _MAX_CALLS + 1:
                        raise InvalidLock("called-distribution limit")
                except InvalidName as error:
                    raise InvalidLock(
                        "invalid installed distribution identity"
                    ) from error
    records: dict[str, list[dict]] = {}
    for record in document["package"]:
        if not isinstance(record, dict) or not isinstance(record.get("name"), str):
            raise InvalidLock("invalid uv package identity")
        if len(record["name"].encode("utf-8")) > _MAX_IDENTITY_BYTES:
            raise InvalidLock("uv package identity limit")
        try:
            name = canonicalize_name(record["name"], validate=True)
        except InvalidName as error:
            raise InvalidLock("invalid uv package identity") from error
        if name in needed:
            records.setdefault(name, []).append(record)
    for name in sorted(needed):
        expected = set()
        for record in records.get(name, ()):
            if not _selected(record, environment):
                continue
            raw_version = record.get("version")
            if not isinstance(raw_version, str):
                raise InvalidLock("called package has no locked version")
            if len(raw_version.encode("utf-8")) > _MAX_IDENTITY_BYTES:
                raise InvalidLock("locked version text limit")
            try:
                expected.add(Version(raw_version))
            except InvalidVersion as error:
                raise InvalidLock("invalid locked package version") from error
        choices = tuple(str(value) for value in sorted(expected))
        if len(expected) != 1:
            findings.append(
                Finding(
                    name,
                    "version_not_locked" if not expected else "ambiguous_lock_version",
                    choices,
                )
            )
            continue
        try:
            actual = version(name)
        except metadata.PackageNotFoundError:
            findings.append(Finding(name, "package_not_installed", choices))
            continue
        if not isinstance(actual, str):
            findings.append(Finding(name, "invalid_installed_version", choices))
            continue
        if len(actual.encode("utf-8")) > _MAX_IDENTITY_BYTES:
            raise InvalidLock("installed version text limit")
        try:
            parsed = Version(actual)
        except InvalidVersion:
            findings.append(Finding(name, "invalid_installed_version", choices, actual))
            continue
        if parsed not in expected:
            findings.append(Finding(name, "version_mismatch", choices, actual))
    return tuple(findings)


class InvalidBinding(ValueError):
    """Installed callable does not have the exact closed named signature."""

    def __init__(self, call, reason, *, missing=(), extra=(), unsupported=()):
        self.call = call
        self.reason = reason
        self.missing = tuple(missing)
        self.extra = tuple(extra)
        self.unsupported = tuple(unsupported)
        super().__init__(reason)


def _concrete_signature(target: object, depth: int = 0) -> inspect.Signature:
    # Ignore writable __signature__/__wrapped__ metadata. Reconstruct actual
    # Python code/formals; native callable signatures use inspect's builtin path.
    if depth >= 64:
        raise ValueError("callable introspection depth limit")
    bound_to = None
    if inspect.ismethod(target):
        function = target.__func__
        bound_to = target.__self__
    elif inspect.isfunction(target):
        function = target
    elif (
        inspect.isbuiltin(target)
        or inspect.ismethoddescriptor(target)
        or inspect.ismethodwrapper(target)
    ):
        return inspect._signature_from_builtin(
            inspect.Signature, target, skip_bound_arg=True
        )
    else:
        descriptor = inspect.getattr_static(type(target), "__call__", None)
        if descriptor is None:
            raise ValueError("callable has no concrete call signature")
        getter = inspect.getattr_static(type(descriptor), "__get__", None)
        bound = (
            descriptor if getter is None else getter(descriptor, target, type(target))
        )
        return _concrete_signature(bound, depth + 1)
    concrete = FunctionType(
        function.__code__,
        function.__globals__,
        function.__name__,
        function.__defaults__,
        function.__closure__,
    )
    concrete.__kwdefaults__ = function.__kwdefaults__
    actual = concrete if bound_to is None else MethodType(concrete, bound_to)
    return inspect.signature(actual, follow_wrapped=False)


def resolve_callable(call: str, parameters: Sequence[str]):
    """Resolve normal installed code after the shared engine verifies its lock.

    This host capability neither runs test cases nor supplies defaults. The engine
    owns both, and will call this already-bound target with exact named scalars.
    Ordinary import/introspection exceptions and interrupts keep their identity.
    """
    if len(call.encode("utf-8")) > _MAX_IDENTITY_BYTES or len(parameters) > _MAX_CALLS:
        raise InvalidBinding(call, "callable metadata limit")
    module, separator, name = call.rpartition(".")
    if not separator or not module or not name:
        raise InvalidBinding(call, "callable must be module-qualified")
    target = getattr(importlib.import_module(module), name)
    if not callable(target):
        raise InvalidBinding(call, "installed member is not callable")
    signature = _concrete_signature(target)
    allowed = (inspect.Parameter.POSITIONAL_OR_KEYWORD, inspect.Parameter.KEYWORD_ONLY)
    actual = {p.name for p in signature.parameters.values() if p.kind in allowed}
    unsupported = tuple(
        (p.name, p.kind.name.lower())
        for p in signature.parameters.values()
        if p.kind not in allowed
    )
    declared = set(parameters)
    if len(declared) != len(parameters) or unsupported or actual != declared:
        raise InvalidBinding(
            call,
            "installed callable signature differs from params",
            missing=sorted(declared - actual),
            extra=sorted(actual - declared),
            unsupported=unsupported,
        )
    return target
