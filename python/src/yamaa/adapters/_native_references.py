"""Name/index translation for the native immutable reference compiler catalog."""

import json

from yamaa.models import RuntimeCondition
from yamaa.odm.bindings import BindingFailure, BoundReference
from yamaa.planning.references import (
    IntermediateFinding,
    KeyInference,
    QualifiedFinding,
    ReferenceCompilerFactory,
    ReferenceFinding,
)


class NativeReferenceLimitError(RuntimeError):
    """Reference compiler resource limits are policy, not language diagnostics."""

    def __init__(self, outcome):
        """Retain exact resource counters from the admitted core request."""
        self.resource = outcome["resource"]
        self.limit = int(outcome["limit"])
        self.required = int(outcome["required"])
        super().__init__(f"native reference analysis {self.resource} limit exceeded")


def _outcome(text, protocol):
    """Decode only trusted native outcomes, preserving explicit policy failures."""
    response = json.loads(text)
    if response["protocol"] != protocol:
        raise ValueError("unsupported native reference response protocol")
    outcome = response["outcome"]
    if outcome["status"] == "limit":
        raise NativeReferenceLimitError(outcome)
    if outcome["status"] != "complete":
        raise ValueError("unknown native reference status")
    return outcome


def bind_reference_compiler(native) -> ReferenceCompilerFactory:
    """Capture catalog construction before project activation or source-provider effects."""
    compile_catalog = getattr(native, "_compile_reference_catalog", None)
    if not callable(compile_catalog):
        raise TypeError("native _compile_reference_catalog must be callable")

    def prepare(bindings, column_types):
        """Snapshot metadata once; actual resolution and output rules remain in Rust."""
        outputs = tuple(column_types)
        positions = {name: index for index, name in enumerate(outputs)}
        datasets = tuple(bindings.datasets)
        dataset_positions = {name: index for index, name in enumerate(datasets)}
        fields = []
        for dataset in datasets:
            # The existing binding catalog reaches the first stored field with
            # a given name. Invalid intermediate shadow declarations still get
            # their language diagnostics from planning, not metadata errors.
            distinct = {}
            for column in bindings.datasets[dataset].columns:
                distinct.setdefault(column.name, column.type)
            fields.append(distinct)
        request = {
            "protocol": "reference-catalog/1",
            "catalog": {
                "outputs": [
                    {"name": name, "type": column_types[name]} for name in outputs
                ],
                "datasets": [
                    {
                        "name": name,
                        "fields": [
                            {"name": field, "type": kind}
                            for field, kind in columns.items()
                        ],
                    }
                    for name, columns in zip(datasets, fields, strict=True)
                ],
            },
        }
        catalog, status = compile_catalog(json.dumps(request, separators=(",", ":")))
        _outcome(status, "reference-catalog/1")
        if catalog is None:
            raise ValueError("native reference compilation returned no catalog")
        invoke = catalog.analyze
        field_names = tuple(tuple(columns) for columns in fields)

        def query(query):
            """Send a bounded query against the captured owned catalog, without fallback."""
            text = invoke(
                json.dumps(
                    {"protocol": "reference-queries/1", "queries": [query]},
                    separators=(",", ":"),
                )
            )
            outcome = _outcome(text, "reference-analysis/1")
            (result,) = outcome["results"]
            return result

        class Compiler:
            """Per-attempt prepared compiler; returned indices only recover host names."""

            def comparable_types(self, left, right):
                """Return the shared declared-type comparison without host coercion."""
                result = query(
                    {"kind": "comparable_types", "left": left, "right": right}
                )
                if result["kind"] != "comparable_types":
                    raise ValueError("unexpected native type comparison result")
                return result["comparable"]

            def infer_keys(self, keys, fields):
                """Send all ordered key names and right metadata; Rust selects the keys."""
                result = query(
                    {
                        "kind": "infer_keys",
                        "keys": keys,
                        "fields": [
                            {"name": name, "type": kind}
                            for name, kind in fields.items()
                        ],
                    }
                )
                if result["kind"] != "key_inference":
                    raise ValueError("unexpected native key inference result")
                inference = result["inference"]
                kind = inference["kind"]
                if kind == "keys":
                    return KeyInference(
                        kind, keys=tuple(keys[i] for i in inference["keys"])
                    )
                if kind == "undeclared_output":
                    return KeyInference(kind, key=keys[inference["key"]])
                if kind == "incompatible":
                    return KeyInference(
                        kind,
                        key=keys[inference["key"]],
                        expected=inference["expected"],
                        actual=inference["actual"],
                    )
                if kind == "no_applicable_keys":
                    return KeyInference(kind)
                raise ValueError("unknown native key inference outcome")

            @staticmethod
            def _intermediate(target):
                """Encode already selected intermediate metadata without deciding visibility."""
                if target is None:
                    return None
                return {
                    "source": {"kind": "self", "fields": target.self_fields}
                    if target.dataset == "SELF"
                    else {"kind": "dataset", "name": target.dataset},
                    "derived": target.derived,
                    "readable": target.readable,
                    "dependencies": target.dependencies,
                }

            @staticmethod
            def _intermediate_findings(result):
                """Preserve ordered findings and dependency declaration indices."""
                if result["kind"] != "intermediate_validation":
                    raise ValueError("unexpected native intermediate reference result")
                return tuple(
                    IntermediateFinding(**item) for item in result["diagnostics"]
                )

            def validate_intermediate(self, field, target):
                """Send a direct visibility query against the owned catalog."""
                return self._intermediate_findings(
                    query(
                        {
                            "kind": "validate_intermediate",
                            "field": field,
                            "target": self._intermediate(target),
                        }
                    )
                )

            def validate_intermediate_read(self, read):
                """Send the normalized donor scope, including missing and SELF targets."""
                return self._intermediate_findings(
                    query(
                        {
                            "kind": "validate_intermediate_read",
                            "read": {
                                "reader": read.reader,
                                "target_name": read.target_name,
                                "target": self._intermediate(read.target),
                                "field": read.field,
                                "donor_dataset": read.donor_dataset,
                                "visible": read.visible,
                            },
                        }
                    )
                )

            def validate_qualified(self, name, expected, scope):
                """Serialize selected scope metadata and preserve ordered core findings."""
                phase = (
                    {"kind": "row", "group_by": scope.group_by}
                    if scope.phase == "row"
                    else {"kind": "column", "groups": scope.groups}
                )
                result = query(
                    {
                        "kind": "validate_qualified",
                        "name": name,
                        "expected": expected,
                        "scope": {
                            "drivers": scope.drivers,
                            "current_driver": scope.current_driver,
                            "reach": scope.reach,
                            "joined": scope.joined,
                            "phase": phase,
                        },
                    }
                )
                if result["kind"] != "qualified_validation":
                    raise ValueError("unexpected native qualified reference result")
                return tuple(
                    QualifiedFinding(**finding) for finding in result["diagnostics"]
                )

            def bind(self, name):
                """Convert a core binding into the existing host binding representation."""
                result = query({"kind": "bind", "name": name})
                if result["kind"] != "binding":
                    raise ValueError("unexpected native reference query result")
                bound = result["binding"]
                if bound is None:
                    return BindingFailure(
                        condition=RuntimeCondition(
                            phase="validation",
                            condition="unknown_field",
                            context={"identifier": name},
                        )
                    )
                if bound["kind"] == "output":
                    return BoundReference(
                        name=name, kind="output", field=outputs[bound["column"]]
                    )
                if bound["kind"] != "dataset":
                    raise ValueError("unknown native reference binding kind")
                dataset = bound["dataset"]
                return BoundReference(
                    name=name,
                    kind="dataset",
                    dataset=datasets[dataset],
                    field=field_names[dataset][bound["field"]],
                )

            def validate_output(self, name, expected, available, candidates):
                """Attach names to core-selected conditions; hosts later attach authored paths."""
                result = query(
                    {
                        "kind": "validate_output",
                        "name": name,
                        "expected": expected,
                        "available": None
                        if available is None
                        else [
                            positions[item] for item in available if item in positions
                        ],
                        "candidates": [
                            dataset_positions[item]
                            for item in candidates
                            if item in dataset_positions
                        ],
                    }
                )
                if result["kind"] != "validation":
                    raise ValueError("unexpected native reference query result")
                finding = result["diagnostic"]
                if finding is None:
                    return None
                condition = finding["condition"]
                if condition == "incompatible_input_type":
                    context = {
                        "source": name,
                        "expected": finding["expected"],
                        "actual": finding["actual"],
                    }
                elif condition == "phase_boundary":
                    context = {
                        "identifier": outputs[finding["column"]],
                        "available_phase": "column_derivation",
                        "required_phase": "row_construction",
                    }
                elif condition == "unresolvable_name":
                    context = {
                        "identifier": name,
                        "suggestion": f"{datasets[finding['dataset']]}.{name}",
                    }
                elif condition == "unknown_field":
                    context = {"identifier": name}
                else:
                    raise ValueError("unknown native reference diagnostic")
                return ReferenceFinding(condition, context)

        return Compiler()

    return prepare
