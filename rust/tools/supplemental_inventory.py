"""Reconcile supplemental contracts and retain actual installed-suite evidence.

Component and compile probes remain separate from complete benchmark runs. Merely
listing a contract or one of its consumers never qualifies its behavior.
"""

from __future__ import annotations

import argparse
import ast
import json
import os
import platform
import re
import subprocess
import sys
import tempfile
from pathlib import Path


def _path(root, name):
    """Resolve a repository-relative identity without allowing it to escape."""
    if not isinstance(name, str) or not name or Path(name).is_absolute():
        raise ValueError(f"invalid relative path: {name!r}")
    path = (root / name).resolve()
    if not path.is_relative_to(root.resolve()):
        raise ValueError(f"path leaves root: {name}")
    return path


def _keys(value, expected):
    if not isinstance(value, dict) or set(value) != set(expected):
        raise ValueError(f"expected exactly these fields: {sorted(expected)}")


def require_native_consumer(source, name):
    """Reject reference semantic imports in retained installed Python suites."""
    prefixes = (
        "yamaa._reference_domain",
        "yamaa.adapters",
        "yamaa.models",
        "yamaa.runtime",
        "yamaa.planning",
        "yamaa.expressions",
        "yamaa.regex",
        "yamaa.specification",
        "yamaa.schema",
        "yamaa.verification",
        "yamaa.submission",
        "yamaa.functions",
    )

    def reference(module):
        return (
            module == "yamaa.io"
            or module.startswith("yamaa.io.")
            or any(
                module == prefix or module.startswith(prefix + ".")
                for prefix in prefixes
            )
        )

    def constant_string(node):
        if isinstance(node, ast.Constant) and isinstance(node.value, str):
            return node.value
        return None

    def constant_int(node):
        if isinstance(node, ast.Constant) and isinstance(node.value, int):
            return node.value
        return None

    def argument(node, index, keyword):
        if len(node.args) > index:
            return node.args[index]
        for item in node.keywords:
            if item.arg == keyword:
                return item.value
        return None

    def resolve_relative(module, package):
        """Resolve leading dots against ``package`` as importlib would.

        Returns ``None`` when the name is relative but no anchor package is
        available: the absolute name is then unknowable statically, and the
        import must not be classified from its remainder alone.
        """
        if not module.startswith("."):
            return module
        if package is None:
            return None
        level = len(module) - len(module.lstrip("."))
        remainder = module[level:]
        base = [part for part in package.split(".") if part]
        if level > 1:
            base = base[: -(level - 1)] if level - 1 <= len(base) else []
        parts = base + ([remainder] if remainder else [])
        return ".".join(parts)

    def globals_package(node):
        """Read ``__package__`` from a literal ``__import__`` globals dict."""
        if not isinstance(node, ast.Dict):
            return None
        for key, value in zip(node.keys, node.values, strict=True):
            if isinstance(key, ast.Constant) and key.value == "__package__":
                return constant_string(value)
        return None

    tree = ast.parse(source, filename=name)
    import_names = ("import_module", "__import__")
    # Local name -> canonical dynamic-import function it is bound to.
    imports = {
        alias.asname or alias.name: alias.name
        for node in ast.walk(tree)
        if isinstance(node, ast.ImportFrom) and node.module in ("importlib", "builtins")
        for alias in node.names
        if alias.name in import_names
    }
    imports["__import__"] = "__import__"

    def import_function_kind(node):
        """Canonical dynamic-import function ``node`` denotes, if any.

        Returns ``"unknown"`` for a ``getattr``/``__dict__`` lookup whose
        key is not a constant string: the function it yields cannot be
        known statically, so a call through it must fail closed rather
        than be classified as not-an-import.
        """
        if isinstance(node, ast.Name):
            return imports.get(node.id)
        if isinstance(node, ast.Attribute):
            return node.attr if node.attr in import_names else None
        if isinstance(node, ast.Call):
            if isinstance(node.func, ast.Name) and node.func.id == "getattr":
                attr = constant_string(node.args[1]) if len(node.args) > 1 else None
                if attr is None:
                    return "unknown"
                return attr if attr in import_names else None
            return None
        if isinstance(node, ast.Subscript):
            key = constant_string(node.slice)
            if key is not None:
                return key if key in import_names else None
            if isinstance(node.value, ast.Attribute) and node.value.attr == "__dict__":
                return "unknown"
            return None
        return None

    # Names assigned a dynamic-import function (directly, via getattr, or
    # via a __dict__ subscript) are themselves dynamic-import functions.
    # Names assigned the saved ``builtins.__import__`` itself are also
    # recorded as guard aliases: the installed suites save it to delegate
    # to from their own runtime import guard (see is_guard_delegation).
    guard_aliases = {
        target.id
        for node in ast.walk(tree)
        if isinstance(node, (ast.Assign, ast.AnnAssign)) and node.value is not None
        for target in (node.targets if isinstance(node, ast.Assign) else [node.target])
        if isinstance(target, ast.Name)
        and (
            (isinstance(node.value, ast.Attribute) and node.value.attr == "__import__")
            or (isinstance(node.value, ast.Name) and node.value.id == "__import__")
        )
    }
    parents = {
        child: node for node in ast.walk(tree) for child in ast.iter_child_nodes(node)
    }

    def is_guard_delegation(node):
        """Whether ``node`` delegates to the saved real ``__import__``.

        The suites' runtime guards have the shape
        ``original = builtins.__import__`` plus
        ``def reject(name, *args, **kwargs): ... return original(name, ...)``:
        the alias is called with the enclosing guard function's own first
        parameter, and the guard itself checks that name at runtime. Only
        that exact shape is exempt from the fail-closed target rule; a
        computed target through the same alias is still rejected.
        """
        if not (isinstance(node.func, ast.Name) and node.func.id in guard_aliases):
            return False
        target_node = argument(node, 0, "name")
        if not isinstance(target_node, ast.Name):
            return False
        enclosing = parents.get(node)
        while enclosing is not None and not isinstance(
            enclosing, (ast.FunctionDef, ast.AsyncFunctionDef, ast.Lambda)
        ):
            enclosing = parents.get(enclosing)
        if enclosing is None:
            return False
        parameters = {
            arg.arg
            for arg in (
                enclosing.args.posonlyargs
                + enclosing.args.args
                + enclosing.args.kwonlyargs
            )
        }
        return target_node.id in parameters

    assignments = [
        node
        for node in ast.walk(tree)
        if isinstance(node, (ast.Assign, ast.AnnAssign)) and node.value is not None
    ]
    changed = True
    while changed:
        changed = False
        for assignment in assignments:
            kind = import_function_kind(assignment.value)
            if kind not in import_names:
                continue
            targets = (
                assignment.targets
                if isinstance(assignment, ast.Assign)
                else [assignment.target]
            )
            for target in targets:
                if isinstance(target, ast.Name) and target.id not in imports:
                    imports[target.id] = kind
                    changed = True

    def dynamic_import_modules(node, kind):
        """Resolve a dynamic import call's target, failing closed.

        A target that is not a constant string (concatenation, f-string,
        variable, starred argument, or no argument at all) cannot be
        classified, so it is rejected instead of silently allowed.
        """
        target_node = argument(node, 0, "name")
        target = constant_string(target_node) if target_node is not None else None
        if target is None:
            raise ValueError(
                f"retained suite has an unresolved dynamic import: {name}:{node.lineno}"
            )
        if kind == "__import__":
            level_node = argument(node, 4, "level")
            if level_node is not None and constant_int(level_node) is None:
                raise ValueError(
                    "retained suite has an unresolved dynamic import: "
                    f"{name}:{node.lineno}"
                )
            level = constant_int(level_node) if level_node is not None else 0
            if level:
                globals_node = argument(node, 1, "globals")
                package = (
                    globals_package(globals_node) if globals_node is not None else None
                )
                resolved = resolve_relative("." * level + target, package)
            else:
                resolved = target
        else:
            package_node = argument(node, 1, "package")
            package = (
                constant_string(package_node) if package_node is not None else None
            )
            resolved = resolve_relative(target, package)
        if resolved is None:
            raise ValueError(
                "retained suite has an unresolved relative dynamic "
                f"import: {name}:{node.lineno}: {target}"
            )
        return [resolved]

    def check_eval_call(node):
        """Check code evaluated from a constant string; fail closed otherwise."""
        code_node = node.args[0] if node.args else None
        code = constant_string(code_node) if code_node is not None else None
        if code is None:
            raise ValueError(
                f"retained suite has an unresolved eval/exec call: {name}:{node.lineno}"
            )
        try:
            ast.parse(code)
        except SyntaxError:
            try:
                ast.parse(code, mode="eval")
            except SyntaxError:
                raise ValueError(
                    "retained suite has an unresolved eval/exec call: "
                    f"{name}:{node.lineno}"
                ) from None
            # A bare expression parses as a statement once parenthesised,
            # so the recursive check can walk its calls the same way.
            code = f"({code})"
        require_native_consumer(code, name)

    for node in ast.walk(tree):
        modules = []
        if isinstance(node, ast.Import):
            modules = [alias.name for alias in node.names]
        elif isinstance(node, ast.ImportFrom):
            modules = [node.module or ""]
            if node.module == "yamaa":
                modules.extend("yamaa." + alias.name for alias in node.names)
        elif isinstance(node, ast.Call):
            function = node.func
            is_eval = (
                isinstance(function, ast.Name) and function.id in ("eval", "exec")
            ) or (
                isinstance(function, ast.Attribute)
                and function.attr in ("eval", "exec")
                and isinstance(function.value, ast.Name)
                and function.value.id == "builtins"
            )
            if is_eval:
                check_eval_call(node)
            else:
                kind = import_function_kind(function)
                if kind == "unknown":
                    raise ValueError(
                        "retained suite has an unresolved dynamic import: "
                        f"{name}:{node.lineno}"
                    )
                if kind is not None and not is_guard_delegation(node):
                    modules = dynamic_import_modules(node, kind)
        for module in modules:
            if reference(module) or module in (
                "yamaa.yamaa_domain",
                "yamaa.DomainRun",
                "yamaa.DomainRunError",
                "yamaa.generate_study_document",
            ):
                raise ValueError(
                    f"retained suite imports reference semantics: {name}:{node.lineno}: {module}"
                )


def load_catalog(root, catalog_path):
    """Reject missing/new contracts and suites instead of freezing the denominator."""
    catalog = json.loads(catalog_path.read_text(encoding="utf-8"))
    _keys(catalog, ("version", "contracts", "suites", "support_files"))
    if catalog["version"] != "1":
        raise ValueError("unsupported supplemental catalog version")
    if not all(
        isinstance(catalog[key], list)
        for key in ("contracts", "suites", "support_files")
    ):
        raise ValueError("contracts, suites and support_files must be lists")
    contracts, suites, ids, staged, supporting = set(), set(), set(), set(), set()
    for contract in catalog["contracts"]:
        _keys(contract, ("path", "family", "level", "consumers"))
        name = contract["path"]
        if name in contracts or not _path(root, name).is_file():
            raise ValueError(f"duplicate or missing contract: {name}")
        contracts.add(name)
        if contract["level"] not in ("component", "compile"):
            raise ValueError(f"invalid supplemental level: {name}")
        if not isinstance(contract["family"], str) or not contract["family"]:
            raise ValueError(f"missing semantic family: {name}")
        consumers = contract["consumers"]
        if (
            not isinstance(consumers, list)
            or not consumers
            or len(set(consumers)) != len(consumers)
        ):
            raise ValueError(f"missing or duplicate contract consumers: {name}")
        for consumer in consumers:
            if not _path(root, consumer).is_file():
                raise ValueError(f"missing contract consumer: {consumer}")
    for suite in catalog["suites"]:
        _keys(suite, ("id", "runtime", "path", "staged_path", "level"))
        if suite["runtime"] not in ("python", "r") or suite["level"] not in (
            "component",
            "compile",
            "shared_run",
        ):
            raise ValueError(f"invalid suite route or level: {suite['id']}")
        if (
            not isinstance(suite["id"], str)
            or re.fullmatch(r"(?:python|r)/[a-z][a-z0-9_-]*", suite["id"]) is None
            or not suite["id"].startswith(suite["runtime"] + "/")
        ):
            raise ValueError("suite identity must be host/simple-name")
        if suite["id"] in ids or suite["path"] in suites:
            raise ValueError(f"duplicate suite: {suite['id']}")
        if not _path(root, suite["path"]).is_file():
            raise ValueError(f"missing suite: {suite['path']}")
        if suite["runtime"] == "python":
            require_native_consumer(
                _path(root, suite["path"]).read_text(encoding="utf-8"), suite["path"]
            )
        _path(root, suite["staged_path"])
        stage = suite["runtime"], suite["staged_path"]
        if stage in staged:
            raise ValueError(f"duplicate staged suite path: {stage}")
        staged.add(stage)
        ids.add(suite["id"])
        suites.add(suite["path"])
    for support in catalog["support_files"]:
        _keys(support, ("path", "role"))
        name = support["path"]
        if name in supporting or name in contracts or not _path(root, name).is_file():
            raise ValueError(f"duplicate or missing support file: {name}")
        if support["role"] not in ("input", "documentation"):
            raise ValueError(f"invalid support file role: {name}")
        supporting.add(name)
    discovered_fixtures = {
        p.relative_to(root).as_posix()
        for pattern in (
            "rust/crates/*/tests/fixtures/**/*",
            "rust/tests/host-fixtures/**/*",
        )
        for p in root.glob(pattern)
        if p.is_file() and "__pycache__" not in p.parts
    }
    discovered_suites = {
        p.relative_to(root).as_posix()
        for pattern in ("rust/tests/installed_*.py", "R/yamaa/tests/*.R")
        for p in root.glob(pattern)
    }
    for label, expected, actual in (
        ("fixture files", contracts | supporting, discovered_fixtures),
        ("suites", suites, discovered_suites),
    ):
        if expected != actual:
            raise ValueError(
                f"{label} differ: uncataloged={sorted(actual - expected)}, stale={sorted(expected - actual)}"
            )
    return catalog


def host_fixture_files(catalog):
    """Return the shared installed-host inputs/codecs, excluding ownership docs."""
    prefixes = (
        ("rust/tests/host-fixtures/", "host-fixtures/"),
        ("rust/crates/yamaa-adapters/tests/fixtures/sas/", "host-fixtures/sas/"),
    )
    names = [contract["path"] for contract in catalog["contracts"]]
    names += [
        support["path"]
        for support in catalog["support_files"]
        if support["role"] == "input"
    ]
    files, staged = [], set()
    for name in names:
        for prefix, destination in prefixes:
            if name.startswith(prefix):
                relative = destination + name.removeprefix(prefix)
                if relative in staged:
                    raise ValueError(f"duplicate staged host fixture path: {relative}")
                staged.add(relative)
                files.append((name, relative))
                break
    return files


def installed_metadata(runtime, staged_root):
    """Read versions and location from the actual installed host before its probes."""
    if runtime == "python":
        code = """
import importlib.metadata as metadata
import json, platform
from pathlib import Path
import yamaa
from yamaa import _native as yamaa_native
for module, name in ((yamaa, "yamaa"), (yamaa_native, "yamaa")):
    distribution = metadata.distribution(name)
    if distribution.files is None or not any(
        distribution.locate_file(f).resolve() == Path(module.__file__).resolve()
        for f in distribution.files
    ):
        raise RuntimeError("module is not owned by installed distribution: " + name)
print(json.dumps({
    "runtime_version": platform.python_version(),
    "host_package_version": metadata.version("yamaa"),
    "binding_package_version": metadata.version("yamaa"),
    "core_version": yamaa_native._CORE_VERSION,
    "package_location": str(Path(yamaa_native.__file__).resolve()),
}))
"""
        return json.loads(
            subprocess.check_output(
                [sys.executable, "-I", "-c", code], cwd=staged_root, text=True
            )
        )
    code = """
suppressPackageStartupMessages(library(yamaa))
writeLines(c(as.character(getRversion()), as.character(packageVersion("yamaa")),
             yamaa:::.core_version(), normalizePath(find.package("yamaa"))))
"""
    # Windows Rscript can fail while forwarding multiline -e expressions.
    # A real script uses the same public calls as the installed R suites.
    with tempfile.TemporaryDirectory(
        prefix="yamaa-r-metadata-", dir=staged_root
    ) as work:
        script = Path(work) / "metadata.R"
        script.write_text(code, encoding="utf-8")
        lines = subprocess.check_output(
            ["Rscript", "--vanilla", str(script)], cwd=staged_root, text=True
        ).splitlines()
    if len(lines) != 4 or not all(lines):
        raise ValueError("R installation metadata was malformed")
    return dict(
        zip(
            (
                "runtime_version",
                "host_package_version",
                "core_version",
                "package_location",
            ),
            lines,
            strict=True,
        )
    )


def run_suites(root, catalog, *, runtime, staged_root, output, evidence):
    """Run existing staged suites in fresh processes and preserve failed/unrun rows."""
    if staged_root.resolve().is_relative_to(root.resolve()):
        raise ValueError("installed suites must run outside the checkout")
    metadata = installed_metadata(runtime, staged_root)
    if Path(metadata["package_location"]).is_relative_to(root.resolve()):
        raise ValueError("installed package resolved into the checkout")
    selected = [s for s in catalog["suites"] if s["runtime"] == runtime]
    # Check all staged script bytes before running the first test. No content digest.
    for suite in selected:
        original = _path(root, suite["path"])
        staged = _path(staged_root, suite["staged_path"])
        if original.read_bytes() != staged.read_bytes():
            raise ValueError(
                f"staged suite differs from tested revision: {suite['id']}"
            )
    for name, staged_name in host_fixture_files(catalog):
        if (
            _path(root, name).read_bytes()
            != _path(staged_root, staged_name).read_bytes()
        ):
            raise ValueError(
                f"staged host fixture differs from tested revision: {name}"
            )
    output.mkdir(parents=True, exist_ok=True)
    environment = os.environ | {
        "YAMAA_SUPPLEMENTAL_EVIDENCE": json.dumps(
            {
                "scope": "supplemental_suites",
                "evidence": evidence,
                "installation": metadata,
            },
            ensure_ascii=True,
        )
    }
    rows, failed = [], False
    for suite in selected:
        row = {
            **suite,
            "backend": "rust",
            "result": "not_exercised",
            "exit_code": None,
            "log": None,
        }
        rows.append(row)
        if failed:
            continue
        log = output / (suite["id"].replace("/", "-") + ".log")
        command = [sys.executable, "-E", "-s"] if runtime == "python" else ["Rscript"]
        command.append(str(_path(staged_root, suite["staged_path"])))
        with log.open("wb") as stream:
            try:
                completed = subprocess.run(
                    command,
                    cwd=staged_root,
                    stdout=stream,
                    stderr=subprocess.STDOUT,
                    check=False,
                    env=environment,
                )
                row["exit_code"] = completed.returncode
                row["result"] = "pass" if completed.returncode == 0 else "failure"
            except OSError as error:
                stream.write(str(error).encode("utf-8"))
                row["result"] = "infrastructure_failure"
        row["log"] = log.name
        failed = row["result"] != "pass"
        print(suite["id"], row["result"], flush=True)
        if failed:
            print(log.read_text(encoding="utf-8", errors="replace"), flush=True)
    report = {
        "version": "1",
        "scope": "supplemental_suites",
        "evidence": evidence,
        "installation": metadata,
        "platform": platform.platform(),
        "catalog": catalog,
        "suites": rows,
    }
    (output / "supplemental.json").write_text(
        json.dumps(report, indent=2) + "\n", encoding="utf-8"
    )
    return int(failed)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True, type=Path)
    parser.add_argument("--catalog", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--runtime", choices=("python", "r"))
    parser.add_argument("--staged-root", type=Path)
    parser.add_argument("--source-revision", required=True)
    parser.add_argument("--artifact-reference")
    parser.add_argument("--host-artifact-reference")
    parser.add_argument("--package-form", choices=("wheel", "source"))
    parser.add_argument("--evidence", required=True)
    args = parser.parse_args(argv)
    root = args.root.resolve()
    actual = subprocess.check_output(
        ["git", "-C", str(root), "rev-parse", "HEAD"], text=True
    ).strip()
    if args.source_revision != actual:
        raise ValueError("source revision differs from current checkout")
    catalog = load_catalog(root, args.catalog)
    evidence = {
        "source_revision": actual,
        "artifact_reference": args.artifact_reference,
        "host_artifact_reference": args.host_artifact_reference,
        "package_form": args.package_form,
        "run": args.evidence,
    }
    if args.runtime:
        if (
            args.staged_root is None
            or not args.artifact_reference
            or args.package_form is None
        ):
            parser.error(
                "suite execution requires staged root, package form and installed artifact reference"
            )
        if args.runtime == "python" and not args.host_artifact_reference:
            parser.error("Python suite execution requires its host artifact reference")
        return run_suites(
            root,
            catalog,
            runtime=args.runtime,
            staged_root=args.staged_root.resolve(),
            output=args.output,
            evidence=evidence,
        )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(
            {
                "version": "1",
                "scope": "supplemental_catalog",
                "evidence": evidence,
                "catalog": catalog,
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
