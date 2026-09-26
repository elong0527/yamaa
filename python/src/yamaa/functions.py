"""Project functions (Stage 2): environment.yaml contracts and bindings."""

import importlib.util
import os

import yaml

from .errors import YamaaError


def _fail(where, phase, condition, requirement=None, context=None):
    raise YamaaError(
        phase=phase,
        condition=condition,
        requirement=requirement,
        spec_paths=[where] if isinstance(where, str) else where,
        context=context or {},
    )


class ProjectFunction:
    """A resolved project function: contract plus lazy Python callable."""

    def __init__(self, name, contract, project_root, call):
        self.name = name
        self.contract = contract
        self.project_root = project_root
        self.call = call
        self.params = {p["name"]: p for p in contract.get("params", [])}
        self.returns = contract.get("returns")
        self.version = contract.get("contract_version")
        self.may_return_missing = contract.get("may_return_missing", False)
        self._implementation = None

    @property
    def implementation(self):
        if self._implementation is None:
            where = f"environment.functions.{self.name}"
            self._implementation = _resolve_binding(
                self.project_root, self.name, self.call, where
            )
        return self._implementation


def load_environment(project_root):
    """Load environment.yaml from project_root. Returns (functions, error)."""
    env_path = os.path.join(project_root, "environment.yaml")
    if not os.path.isfile(env_path):
        _fail(
            "environment",
            "validation",
            "project_environment_missing",
            "REQ-0694",
            {"project_root": project_root},
        )
    with open(env_path, "r", encoding="utf-8") as f:
        env = yaml.safe_load(f)
    if not isinstance(env, dict):
        _fail(
            "environment",
            "validation",
            "project_environment_invalid",
            "REQ-0695",
            {"reason": "not a mapping"},
        )
    runtime = env.get("runtime") or {}
    language = runtime.get("language")
    if language != "python":
        _fail(
            "environment.runtime.language",
            "validation",
            "runner_language_mismatch",
            "REQ-0696",
            {"language": language, "supported": ["python"]},
        )
    functions = {}
    for name, decl in (env.get("functions") or {}).items():
        functions[name] = _load_function(project_root, name, decl)
    return functions


def _load_function(project_root, name, decl):
    """Load a single function declaration: contract + binding."""
    where = f"environment.functions.{name}"
    if not isinstance(decl, dict):
        _fail(
            where,
            "validation",
            "project_environment_invalid",
            "REQ-0695",
            {"function": name, "reason": "not a mapping"},
        )
    # Contract: either inline or a path to a contracts.yaml file.
    contract = None
    if decl.get("contract"):
        contract_path = os.path.join(project_root, decl["contract"])
        if not os.path.isfile(contract_path):
            _fail(
                where + ".contract",
                "validation",
                "project_environment_invalid",
                "REQ-0695",
                {"function": name, "path": decl["contract"]},
            )
        with open(contract_path, "r", encoding="utf-8") as f:
            contracts = yaml.safe_load(f)
        contract = (contracts or {}).get(name)
        if contract is None:
            _fail(
                where + ".contract",
                "validation",
                "project_environment_invalid",
                "REQ-0695",
                {"function": name, "reason": "not in contracts file"},
            )
    else:
        # Inline contract (contract_version, params, returns at decl level).
        contract = {
            "contract_version": decl.get("contract_version"),
            "params": decl.get("params", []),
            "returns": decl.get("returns"),
            "may_return_missing": decl.get("may_return_missing", False),
        }
    if not isinstance(contract, dict):
        _fail(
            where,
            "validation",
            "project_environment_invalid",
            "REQ-0695",
            {"function": name},
        )
    # Binding: "module.function" -> Python callable (resolved lazily on
    # first call, so validation-only benchmarks need no implementation).
    binding = decl.get("binding") or {}
    call = binding.get("call")
    if not call or not isinstance(call, str):
        _fail(
            where + ".binding.call",
            "validation",
            "project_environment_invalid",
            "REQ-0695",
            {"function": name},
        )
    return ProjectFunction(name, contract, project_root, call)


def _resolve_binding(project_root, name, call, where):
    """Resolve "module.function" to a Python callable."""
    if "::" in call:
        # R-style package::function: not supported by the Python engine.
        _fail(
            where + ".binding.call",
            "validation",
            "runner_language_mismatch",
            "REQ-0696",
            {"function": name, "call": call},
        )
    if "." not in call:
        _fail(
            where + ".binding.call",
            "validation",
            "project_environment_invalid",
            "REQ-0695",
            {"function": name, "call": call},
        )
    mod_name, func_name = call.rsplit(".", 1)
    mod_path = os.path.join(project_root, "runtime", mod_name + ".py")
    if not os.path.isfile(mod_path):
        _fail(
            where + ".binding.call",
            "validation",
            "project_environment_invalid",
            "REQ-0695",
            {"function": name, "module": mod_name},
        )
    spec = importlib.util.spec_from_file_location(mod_name, mod_path)
    module = importlib.util.module_from_spec(spec)
    try:
        spec.loader.exec_module(module)
    except Exception as exc:  # noqa: BLE001 -- any load failure is reported
        _fail(
            where + ".binding.call",
            "validation",
            "function_call_failed",
            "REQ-0701",
            {"function": name, "error": str(exc)},
        )
    func = getattr(module, func_name, None)
    if not callable(func):
        _fail(
            where + ".binding.call",
            "validation",
            "project_environment_invalid",
            "REQ-0695",
            {"function": name, "call": call},
        )
    return func


def check_call(functions, payload, where):
    """Validate a function: call site against the environment."""
    name = payload.get("name")
    if name not in functions:
        _fail(
            where + ".name",
            "validation",
            "unknown_project_function",
            "REQ-0698",
            {"function": name},
        )
    func = functions[name]
    requested = payload.get("contract_version")
    if requested != func.version:
        _fail(
            where + ".contract_version",
            "validation",
            "function_contract_mismatch",
            "REQ-0699",
            {"function": name, "requested": requested, "available": func.version},
        )
    args = payload.get("args") or {}
    # REQ-0700: unknown args, missing required args.
    for arg_name in args:
        if arg_name not in func.params:
            _fail(
                where + ".args." + arg_name,
                "validation",
                "invalid_function_argument",
                "REQ-0700",
                {"function": name, "argument": arg_name, "reason": "unknown argument"},
            )
    for param_name, param in func.params.items():
        if (
            param.get("required", True)
            and param_name not in args
            and "default" not in param
        ):
            _fail(
                where + ".args",
                "validation",
                "invalid_function_argument",
                "REQ-0700",
                {
                    "function": name,
                    "argument": param_name,
                    "reason": "missing required argument",
                },
            )
    return func


def call_function(func, arg_values, where):
    """Invoke a project function with resolved arg values."""
    # Short-circuit: missing required arg -> missing result.
    for param_name, param in func.params.items():
        if param.get("required", True) and arg_values.get(param_name) is None:
            return None
    # Apply defaults for optional params not supplied.
    call_args = dict(arg_values)
    for param_name, param in func.params.items():
        if param_name not in call_args and "default" in param:
            call_args[param_name] = param["default"]
    try:
        result = func.implementation(**call_args)
    except Exception as exc:  # noqa: BLE001 -- any call failure is reported
        _fail(
            where,
            "derivation",
            "function_call_failed",
            "REQ-0701",
            {"function": func.name, "error": str(exc)},
        )
    # REQ-0008/REQ-0673: non-finite floats normalize to missing before any
    # consumer observes them. A binding returning non-finite satisfies its
    # contract only if may_return_missing is true.
    if isinstance(result, float) and (
        result != result  # noqa: PLR0124 -- NaN check is the intent
        or result in (float("inf"), float("-inf"))
    ):
        if not func.may_return_missing:
            _fail(
                where,
                "derivation",
                "invalid_function_result",
                "REQ-0702",
                {"function": func.name, "reason": "non-finite result not permitted"},
            )
        return None
    # REQ-0702: validate result type.
    if result is None:
        if not func.may_return_missing:
            _fail(
                where,
                "derivation",
                "invalid_function_result",
                "REQ-0702",
                {"function": func.name, "reason": "unexpected missing result"},
            )
        return None
    expected = func.returns
    if expected == "float":
        if not isinstance(result, (int, float)) or isinstance(result, bool):
            _fail(
                where,
                "derivation",
                "invalid_function_result",
                "REQ-0702",
                {"function": func.name, "reason": "expected float"},
            )
        return float(result)
    if expected == "int":
        if not isinstance(result, int) or isinstance(result, bool):
            _fail(
                where,
                "derivation",
                "invalid_function_result",
                "REQ-0702",
                {"function": func.name, "reason": "expected int"},
            )
        return result
    if expected == "str":
        if not isinstance(result, str):
            _fail(
                where,
                "derivation",
                "invalid_function_result",
                "REQ-0702",
                {"function": func.name, "reason": "expected str"},
            )
        return result
    if expected == "bool":
        if not isinstance(result, bool):
            _fail(
                where,
                "derivation",
                "invalid_function_result",
                "REQ-0702",
                {"function": func.name, "reason": "expected bool"},
            )
        return result
    return result
