"""Installed interface handshake and native gate (rust/interface/README.md).

The generated `yamaa/__init__.py` negotiates right after importing
`yamaa._native`, before any other native call, and every other native
function refuses until a negotiation is accepted. The generated request,
each crafted request's outcome, the message template of each reason and the
gate's refusal are the shared truth's `interface` family, which R reads too;
each outcome was written by hand from the handshake's check order.
"""

import json
import pickle
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from host_fixtures import load as load_truth

import yamaa
from yamaa import _native

TRUTH = load_truth(__file__, "interface")
FIELDS = (
    "accepted",
    "requested_version",
    "supported_version",
    "reason",
    "missing_features",
)
# The native module's boundary version, which a refusal names; the generated
# request must ask for exactly it.
VERSION = _native._interface_handshake({})["supported_version"]


def resolved(value):
    """The truth with its placeholders filled: `{version}` is the boundary
    version, `{next_version}` the one after it and `{core_version}` the
    installed engine's release."""
    if isinstance(value, list):
        return [resolved(item) for item in value]
    if isinstance(value, dict):
        return {key: resolved(item) for key, item in value.items()}
    if isinstance(value, str):
        return {
            "{version}": VERSION,
            "{next_version}": VERSION + 1,
            "{core_version}": _native._CORE_VERSION,
        }.get(value, value)
    return value


def case(name):
    """The crafted case of the truth named `name`, its placeholders filled."""
    [found] = [case for case in TRUTH["cases"] if case["case"] == name]
    return resolved(found)


NEXT_VERSION_CASE = "a request for the next version is a mismatch"


def message(outcome):
    """The truth's message for a refusal: the template of its reason, with
    each field it names filled from the envelope."""
    text = TRUTH["messages"][outcome["reason"]]
    for field in ("requested_version", "supported_version"):
        if outcome[field] is not None:
            text = text.replace("{" + field + "}", str(outcome[field]))
    return text.replace("{missing_features}", ", ".join(outcome["missing_features"]))


def refused(requested, reason):
    return {
        "accepted": False,
        "requested_version": requested,
        "supported_version": VERSION,
        "reason": reason,
        "missing_features": [],
    }


def python_cases():
    """Requests that only Python builds, each with its authored outcome: a
    float is no integer, a key that is not text names no field but leaves the
    version to be read first, text is no list, a tuple is a sequence as a list
    is, and nothing deeper than a request's own shape is read."""
    required = TRUTH["request"]["required"]
    accepted = case("every required identifier and no optional one")["outcome"]
    deep = []
    for _ in range(100_000):
        deep = [deep]
    return [
        (
            "a float version is malformed",
            {"version": float(VERSION), "required": required, "optional": []},
            refused(None, "malformed_request"),
        ),
        (
            "a mapping with a key that is not text is malformed",
            {"version": VERSION, "required": required, "optional": [], 1: []},
            refused(VERSION, "malformed_request"),
        ),
        (
            "another version with a key that is not text is a mismatch",
            {"version": VERSION + 1, 1: []},
            refused(VERSION + 1, "version_mismatch"),
        ),
        (
            "a list nested a hundred thousand deep is malformed",
            {"version": VERSION, "required": deep, "optional": []},
            refused(VERSION, "malformed_request"),
        ),
        (
            "text where a list belongs is malformed",
            {"version": VERSION, "required": required[0], "optional": []},
            refused(VERSION, "malformed_request"),
        ),
        (
            "a tuple of identifiers is a list",
            {"version": VERSION, "required": tuple(required), "optional": ()},
            accepted,
        ),
    ]


def isolated(code, *arguments, cwd):
    """Run `code` in a fresh isolated interpreter, whose import is the
    subject; return the JSON it prints."""
    completed = subprocess.run(
        [sys.executable, "-I", "-c", code, *arguments],
        cwd=cwd,
        capture_output=True,
        text=True,
        check=False,
    )
    if completed.returncode:
        raise AssertionError(completed.stderr)
    return json.loads(completed.stdout)


# Import the package from the directory argv[1] names and report its refusal.
IMPORT = f"""
import json, sys
sys.path.insert(0, sys.argv[1])
try:
    import yamaa
except ImportError as error:
    native = sys.modules.get("yamaa._native")
    print(json.dumps({{
        "error": type(error).__name__,
        "import_error": isinstance(error, ImportError),
        "message": str(error),
        "fields": {{field: getattr(error, field) for field in {FIELDS!r}}},
        "calls": getattr(native, "calls", None),
    }}))
else:
    print(json.dumps({{"error": None}}))
"""

# Load the native module alone from argv[1], with no package, and call each
# function of argv[2] with its arguments before negotiation, after a refused
# request and after an accepted one (argv[3]).
NATIVE_ALONE = """
import importlib.machinery, importlib.util, json, sys
path, calls, requests = sys.argv[1], json.loads(sys.argv[2]), json.loads(sys.argv[3])
loader = importlib.machinery.ExtensionFileLoader("yamaa._native", path)
native = importlib.util.module_from_spec(importlib.util.spec_from_loader("yamaa._native", loader))
loader.exec_module(native)

def outcome(name):
    try:
        getattr(native, name)(*calls[name])
    except Exception as error:
        return [type(error).__name__, str(error)]
    return ["returned", None]

phases = {"functions": sorted(
    name for name, value in vars(native).items()
    if callable(value) and not isinstance(value, type)
)}
phases["before"] = {name: outcome(name) for name in calls}
native._interface_handshake(requests["refused"])
phases["after a refusal"] = {name: outcome(name) for name in calls}
native._interface_handshake(requests["accepted"])
phases["after an acceptance"] = {name: outcome(name) for name in calls}
print(json.dumps(phases))
"""

# Each native function but the handshake, with arguments of its types.
CALLS = {
    "_table_from_pickle": [None],
    "_plans": [],
    "_spec_check": ["missing.yaml"],
    "_spec_summary": ["missing.yaml"],
    "_spec_run": ["missing.yaml"],
    "_spec_resolve": ["missing.yaml"],
    "_project_spec": ["missing.yaml", "TEST"],
    "_project_summary": ["missing.yaml"],
    "_project_names": ["missing.yaml"],
    "_project_check": ["missing.yaml"],
    "_project_run": ["missing.yaml"],
    "_project_submission": ["missing.yaml"],
    "_read_odm": ["missing.xml"],
    "_lint": [["missing.yaml"]],
    "_schema": ["spec"],
    "_conditions": [],
    "_info": [],
    "_selftest": [],
    "_live_handles": [],
}

# A native module from before the handshake: it has no _interface_handshake.
WITHOUT_HANDSHAKE = '"""A stub native module without the handshake."""\n'
# A native module of the next boundary version, which records every call.
NEXT_VERSION = '''"""A stub native module of the next boundary version."""
calls = []


def _interface_handshake(request):
    calls.append("_interface_handshake")
    return {
        "accepted": False,
        "requested_version": request["version"],
        "supported_version": request["version"] + 1,
        "reason": "version_mismatch",
        "missing_features": [],
    }


def __getattr__(name):
    if not name.startswith("__"):
        calls.append(name)
    raise AttributeError(name)
'''


class Interface(unittest.TestCase):
    def setUp(self):
        work = tempfile.TemporaryDirectory(prefix="yamaa-interface-")
        self.addCleanup(work.cleanup)
        self.root = Path(work.name)

    def import_over(self, native):
        """Import a copy of the installed package's Python modules over the
        stub native module `native`, in a fresh interpreter."""
        package = self.root / "packages" / "yamaa"
        package.mkdir(parents=True, exist_ok=True)
        for module in Path(yamaa.__file__).parent.glob("*.py"):
            shutil.copy2(module, package / module.name)
        (package / "_native.py").write_text(native, encoding="utf-8")
        return isolated(IMPORT, str(package.parent), cwd=self.root)

    def test_import_negotiates_the_generated_request(self):
        accepted = _native._interface_handshake(yamaa._INTERFACE_REQUEST)
        formats = [
            name.removeprefix("format_")
            for name in accepted["enabled"]
            if name.startswith("format_")
        ]
        self.assertEqual(
            (yamaa._INTERFACE_REQUEST, accepted, sorted(formats)),
            (
                resolved(TRUTH["request"]),
                resolved(TRUTH["accepted"]),
                sorted(yamaa.info()["features"]),
            ),
        )

    def test_each_crafted_request_gets_its_authored_outcome(self):
        shared = [
            (crafted["case"], crafted["request"], crafted["outcome"])
            for crafted in resolved(TRUTH["cases"])
        ]
        for name, request, expected in shared + python_cases():
            with self.subTest(case=name):
                outcome = _native._interface_handshake(request)
                self.assertEqual(outcome, expected)
                if not expected["accepted"]:
                    # An unpickled error is made again from its refusal.
                    error = yamaa.InterfaceError(outcome)
                    copied = pickle.loads(pickle.dumps(error))
                    self.assertEqual(
                        [
                            (
                                isinstance(each, ImportError),
                                str(each),
                                {field: getattr(each, field) for field in FIELDS},
                            )
                            for each in (error, copied)
                        ],
                        [(True, message(expected), expected)] * 2,
                        f"{name}, as the error and as its unpickled copy",
                    )

    def test_a_native_module_without_the_handshake_refuses_import_as_missing_interface(
        self,
    ):
        fields = {
            "accepted": False,
            "requested_version": VERSION,
            "supported_version": None,
            "reason": "missing_interface",
            "missing_features": [],
        }
        self.assertEqual(
            self.import_over(WITHOUT_HANDSHAKE),
            {
                "error": "InterfaceError",
                "import_error": True,
                "message": message(fields),
                "fields": fields,
                "calls": None,
            },
        )

    def test_a_refused_handshake_stops_import_before_any_other_native_call(self):
        # Import over both an older module without table pickling and a newer
        # module: neither may be used after a refused negotiation.
        for offset in (-1, 1):
            with self.subTest(supported_version=VERSION + offset):
                fields = {
                    "accepted": False,
                    "requested_version": VERSION,
                    "supported_version": VERSION + offset,
                    "reason": "version_mismatch",
                    "missing_features": [],
                }
                native = NEXT_VERSION.replace(
                    'request["version"] + 1', f'request["version"] + {offset}'
                )
                self.assertEqual(
                    self.import_over(native),
                    {
                        "error": "InterfaceError",
                        "import_error": True,
                        "message": message(fields),
                        "fields": fields,
                        "calls": ["_interface_handshake"],
                    },
                )

    def test_every_native_function_refuses_before_negotiation(self):
        requests = {
            "refused": case(NEXT_VERSION_CASE)["request"],
            "accepted": yamaa._INTERFACE_REQUEST,
        }
        self.assertEqual(requests["refused"]["version"], VERSION + 1)
        phases = isolated(
            NATIVE_ALONE,
            _native.__file__,
            json.dumps(CALLS),
            json.dumps(requests),
            cwd=self.root,
        )
        refusal = ["RuntimeError", TRUTH["gate_message"]]
        gated = {
            name: result
            for name, result in phases["after an acceptance"].items()
            if result == refusal
        }
        self.assertEqual(
            (
                phases["functions"],
                phases["before"],
                phases["after a refusal"],
                gated,
            ),
            (
                sorted([*CALLS, "_interface_handshake"]),
                dict.fromkeys(CALLS, refusal),
                dict.fromkeys(CALLS, refusal),
                {},
            ),
        )

    def test_a_direct_handshake_call_leaves_the_negotiation_unchanged(self):
        refusal = _native._interface_handshake(case(NEXT_VERSION_CASE)["request"])
        self.assertEqual(
            (refusal["reason"], _native._spec_summary("spec.yaml")),
            ("version_mismatch", "yamaa spec spec.yaml"),
        )


if __name__ == "__main__":
    unittest.main()
