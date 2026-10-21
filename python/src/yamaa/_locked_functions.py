"""Host metadata for locked environment activation.

The shared engine reads a function's uv lock and compares versions. This module
only reports what the interpreter has installed: the distributions that provide
each called module, a distribution's installed version and the marker
environment. It does not import project code, construct a semantic plan,
install anything or cache activation success, and it imports no third-party
module.
"""

from __future__ import annotations

import importlib
import inspect
import json
import os
import platform
import sys
from collections.abc import Mapping, Sequence
from importlib import metadata
from pathlib import Path
from types import FunctionType, MethodType
from urllib.parse import urlsplit
from urllib.request import url2pathname


class InvalidMetadata(ValueError):
    """Installed metadata cannot identify the distribution of a called module."""


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
    for distribution in metadata.distributions():
        try:
            raw = distribution.read_text("direct_url.json")
        except UnicodeDecodeError as error:
            raise InvalidMetadata("invalid editable metadata") from error
        if raw is None:
            continue
        try:
            direct = json.loads(raw)
        except (ValueError, RecursionError) as error:
            raise InvalidMetadata("invalid editable metadata") from error
        if (
            not isinstance(direct, dict)
            or not isinstance(direct.get("dir_info"), dict)
            or direct["dir_info"].get("editable") is not True
        ):
            continue
        url = direct.get("url")
        if not isinstance(url, str):
            raise InvalidMetadata("invalid editable source identity")
        parsed = urlsplit(url)
        if (
            parsed.scheme != "file"
            or parsed.netloc not in ("", "localhost")
            or parsed.query
            or parsed.fragment
        ):
            raise InvalidMetadata("editable source is not a local file URL")
        project = Path(url2pathname(parsed.path)).resolve()
        name = distribution.metadata.get("Name")
        if not isinstance(name, str):
            raise InvalidMetadata("invalid editable distribution identity")
        for entry in distribution.files or ():
            if entry.suffix != ".pth":
                continue
            installed = Path(entry.locate())
            try:
                held = installed.read_bytes()
            except FileNotFoundError:
                # RECORD can outlive an editable path file. Its absence cannot
                # identify a provider or obstruct other called distributions.
                continue
            try:
                lines = held.decode("utf-8").splitlines()
            except UnicodeDecodeError as error:
                raise InvalidMetadata("invalid editable path metadata") from error
            for line in lines:
                line = line.strip()
                if not line or line.startswith(("#", "import ", "import\t")):
                    continue
                path = (installed.parent / line).resolve()
                if not path.is_relative_to(project):
                    continue
                for root in roots:
                    if (path / root).is_dir() or (path / (root + ".py")).is_file():
                        names = mapping.setdefault(root, [])
                        if name not in names:
                            names.append(name)
    return mapping


def is_standard_library(module: str) -> bool:
    """Whether `module` names a module of the interpreter's standard library."""
    return module in sys.stdlib_module_names


def installed_version(distribution: str) -> object:
    """The installed version of `distribution`, as its metadata records it.

    The engine reports a value that is not text as an unreadable version, and
    `metadata.PackageNotFoundError` as a distribution that is not installed.
    Every other exception, and an interrupt, keeps its identity.
    """
    return metadata.version(distribution)


def integer_digits() -> int:
    """The interpreter's limit on the digits of an integer read from text, 0 when off.

    packaging converted every number of a version with `int()`, under this limit,
    so the engine applies it to the versions it compares.
    """
    return sys.get_int_max_str_digits()


def marker_environment() -> dict[str, str]:
    """The values that PEP 508 markers name, read as Python's packaging reads them.

    The engine evaluates a lock's `resolution-markers` against them.
    """
    implementation = sys.implementation.version
    version = f"{implementation.major}.{implementation.minor}.{implementation.micro}"
    if implementation.releaselevel != "final":
        version += implementation.releaselevel[0] + str(implementation.serial)
    return {
        "implementation_name": sys.implementation.name,
        "implementation_version": version,
        "os_name": os.name,
        "platform_machine": platform.machine(),
        "platform_release": platform.release(),
        "platform_system": platform.system(),
        "platform_version": platform.version(),
        "python_full_version": platform.python_version(),
        "platform_python_implementation": platform.python_implementation(),
        "python_version": ".".join(platform.python_version_tuple()[:2]),
        "sys_platform": sys.platform,
    }


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
        if not inspect.isfunction(function):
            raise ValueError("bound callable has no concrete Python formals")
    elif inspect.isfunction(target):
        function = target
    elif (
        inspect.isbuiltin(target)
        or inspect.ismethoddescriptor(target)
        or inspect.ismethodwrapper(target)
    ):
        # inspect.signature would honour a writable __signature__; this is the
        # stdlib's own builtin path, present on every supported CPython.
        return inspect._signature_from_builtin(  # pyrefly: ignore[missing-attribute]
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
    Import exceptions and interrupts keep their identity. Unavailable concrete
    signatures are binding failures, with the original exception as their cause.
    """
    module, separator, name = call.rpartition(".")
    if not separator or not module or not name:
        raise InvalidBinding(call, "callable must be module-qualified")
    target = getattr(importlib.import_module(module), name)
    if not callable(target):
        raise InvalidBinding(call, "installed member is not callable")
    try:
        signature = _concrete_signature(target)
    except (ValueError, TypeError) as error:
        raise InvalidBinding(
            call, "installed callable has no concrete call signature"
        ) from error
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


def check_registry_callable(target, call, parameters):
    """Validate one caller-supplied registry callable without importing.

    The same concrete-signature rules as `resolve_callable` apply; the
    only difference is the source of the callable: an explicit registry
    mapping supplied by the caller instead of an installed module member.
    """
    if not callable(target):
        raise InvalidBinding(call, "registered member is not callable")
    try:
        signature = _concrete_signature(target)
    except (ValueError, TypeError) as error:
        raise InvalidBinding(
            call, "registered callable has no concrete call signature"
        ) from error
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
            "registered callable signature differs from params",
            missing=sorted(declared - actual),
            extra=sorted(actual - declared),
            unsupported=unsupported,
        )
    return target


def resolve_registry_callable(registry, call, parameters):
    """Resolve one binding from a caller-supplied registry mapping.

    A missing binding is a refusal, never a fallback to module import.
    """
    try:
        target = registry[call]
    except KeyError:
        raise InvalidBinding(call, "registered binding is missing") from None
    return check_registry_callable(target, call, parameters)


def invoke_batch_callable(target, columns):
    """Call a vectorized registry callable once with named columns.

    Every column must have the same length; the callable must return a
    list or tuple of exactly that length. Anything else is refused here,
    and the shared engine re-validates length and exact result types
    before accepting any output.

    A column is a list or tuple of row values, mirroring the return rule
    below: a string or bytes column would otherwise be split into its
    characters or byte values and silently read as one row per element,
    and a non-sequence column leaked a raw TypeError from ``len`` instead
    of the InvalidBinding every other batch refusal uses.
    """
    if not isinstance(columns, Mapping):
        raise InvalidBinding("batch", "batch columns must be a mapping")
    for column in columns.values():
        if isinstance(column, (str, bytes)) or not isinstance(column, (list, tuple)):
            raise InvalidBinding("batch", "batch column is not a sequence")
    lengths = {len(column) for column in columns.values()}
    if len(lengths) > 1:
        raise InvalidBinding("batch", "batch columns differ in length")
    rows = lengths.pop() if lengths else 0
    returned = target(**{name: list(column) for name, column in columns.items()})
    if isinstance(returned, (str, bytes)) or not isinstance(returned, (list, tuple)):
        raise InvalidBinding("batch", "batch callable returned no sequence")
    if len(returned) != rows:
        raise InvalidBinding("batch", "batch callable returned the wrong length")
    return list(returned)
