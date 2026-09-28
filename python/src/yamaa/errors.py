"""Structured run failure. Carries phase/condition/requirement/spec_paths
so the benchmark runner can match expected/error.yaml entries."""

import json


class YamaaError(Exception):
    def __init__(
        self, phase, condition, requirement=None, spec_paths=None, context=None
    ):
        self.phase = phase
        self.condition = condition
        self.requirement = requirement
        self.spec_paths = list(spec_paths or [])
        self.context = context or {}
        super().__init__(
            f"[{phase}/{condition}]"
            + (f" {requirement}" if requirement else "")
            + (f" at {' ,'.join(self.spec_paths)}" if self.spec_paths else "")
            + (f" :: {json.dumps(self.context, default=str)}" if self.context else "")
        )
