"""Observe the bounded native backend without reference execution or fallback.

Loading and semantic planning still use the explicit temporary Python bridge.
This runner does not qualify shared compilation or producer workflows.
"""

from __future__ import annotations

import json
from dataclasses import replace
from pathlib import Path

from yamaa.adapters._native_project_functions import activate_project
from yamaa.adapters.conformance import (
    HandlerObservation,
    NodeObservation,
    UnsupportedObservation,
    _observe_artifact,
    _observe_diagnostic,
    _published,
    _report,
)
from yamaa.adapters.native_datasets import _execute
from yamaa.adapters.observations import (
    CallbackObservation,
    ObservedResources,
    RunObservations,
    observe_scalar,
)
from yamaa.application import prepare_workflow
from yamaa.functions import select_project_root
from yamaa.io import load_source_tables
from yamaa.odm.items import odm_inputs
from yamaa.planning.workflow import _layer_origins
from yamaa.runtime import ExecutionFailure, ExecutionSuccess
from yamaa.specification import SpecificationError


def execute_native_example(name, entry, schema_root, destination):
    """Execute the installed native dataset bridge once and retain run evidence."""
    observer = RunObservations(entry)
    tables = []
    core_version = "unavailable"
    activation_started = False
    activation_finished = False

    def report(outcome, **observations):
        return _report(
            name,
            outcome,
            backend="rust",
            engine_version=core_version,
            tables=tuple(tables),
            source_reads=tuple(observer.source_reads),
            callbacks=tuple(observer.callbacks),
            verifications=tuple(observer.verifications),
            **observations,
        )

    try:
        import yamaa_native

        core_version = yamaa_native.engine_info()["core_version"]
        try:
            prepared = prepare_workflow(entry, schema_root=schema_root)
        except SpecificationError as error:
            return report(
                "failure",
                diagnostics=tuple(_observe_diagnostic(d) for d in error.diagnostics),
            )
        workflow = prepared.workflow
        if len(workflow.nodes) != 1 or workflow.links:
            return report(
                "unsupported",
                unsupported=(
                    UnsupportedObservation(
                        operation="native_producer_workflow",
                        spec_path="$",
                    ),
                ),
            )
        node = workflow.nodes[0]
        specification = node.resolved.specification
        resources = ObservedResources(prepared.resources, observer, entry.parent)

        def provide(declarations):
            loaded = load_source_tables(
                declarations,
                resources,
                origins=_layer_origins(node.resolved, declarations),
                odm_datasets=odm_inputs(specification),
            )
            for source_name, source in loaded.items():
                tables.append(
                    observer.table(entry, "source", source_name, source.table)
                )
            return loaded

        environment_root = select_project_root(entry.parent)

        def prepare_functions(specification):
            nonlocal activation_started, activation_finished
            activation_started = True
            # Vectors finish before wrapping callbacks, including on repeated runs.
            bindings = activate_project(
                specification, environment_root, schema_root, None, None
            )
            activation_finished = True
            signatures = json.loads(bindings.declarations)

            def observed(signature, callback):
                def invoke(**arguments):
                    observer.callbacks.append(
                        CallbackObservation(
                            specification=observer.current,
                            function=signature["identity"]["name"],
                            contract_version=signature["identity"]["contract_version"],
                            arguments=tuple(
                                (
                                    parameter["name"],
                                    observe_scalar(arguments[parameter["host_name"]]),
                                )
                                for parameter in signature["parameters"]
                            ),
                        )
                    )
                    return callback(**arguments)

                return invoke

            return replace(
                bindings,
                callbacks=tuple(
                    observed(signature, callback)
                    for signature, callback in zip(
                        signatures, bindings.callbacks, strict=True
                    )
                ),
            )

        run = _execute(
            specification,
            provide,
            prepare_functions if environment_root is not None else None,
        )
        result = run.result
        observer.record_verifications(run.verifications)
        diagnostics = tuple(
            _observe_diagnostic(item) for item in getattr(result, "diagnostics", ())
        )
        unsupported = tuple(
            UnsupportedObservation(operation=item.operation, spec_path=item.spec_path)
            for item in getattr(result, "features", ())
        )
        counts = tuple(
            HandlerObservation(spec_path=c.spec_path, handler=c.handler, count=c.count)
            for c in result.handler_counts
        )
        nodes = (
            NodeObservation(
                specification=observer.current,
                outcome=result.status,
                diagnostics=diagnostics,
                unsupported=unsupported,
                handler_counts=counts,
            ),
        )
        if activation_started and not activation_finished:
            nodes = ()
        artifacts = []
        if isinstance(result, ExecutionSuccess):
            tables.append(observer.table(entry, "derived", "output", result.table))
            artifacts = _published(specification.output, result, destination)
        elif isinstance(result, ExecutionFailure):
            output = specification.output
            if (
                result.verification_log is not None
                and output.verification_log is not None
            ):
                artifacts.append(
                    _observe_artifact(
                        Path(output.verification_log).stem,
                        result.verification_log,
                        destination / Path(output.verification_log).name,
                    )
                )
        return report(
            result.status,
            nodes=nodes,
            artifacts=tuple(artifacts),
            diagnostics=diagnostics,
            unsupported=unsupported,
            handler_counts=counts,
        )
    except Exception as error:  # noqa: BLE001 - retain partial evidence of infrastructure failure
        return report("error", error=f"{type(error).__name__}: {error}")
