"""Installed timing probes must preserve existing independent output and effect truth."""

import json
import unittest
from contextlib import ExitStack
from unittest.mock import patch

import installed_datasets
import installed_project_functions
import yamaa_native


class InstalledProfile(unittest.TestCase):
    """Replay existing positive/negative corpora through the actual instrumented entrypoint."""

    def replay(self, case, entrypoints):
        """Time each actual call once, preserving corpus assertions and original interruptions."""
        profiles = []
        omitted = object()

        def execute(request, source, secondary=omitted, callbacks=omitted):
            """Keep timing metadata separate from the existing dataset/1 result contract."""
            table, outcome, encoded = yamaa_native._profile_dataset_functions(
                request,
                source,
                [] if secondary is omitted else secondary,
                [] if callbacks is omitted else callbacks,
            )
            profile = json.loads(encoded)
            self.assertEqual(set(profile), {"protocol", "total_ns", "phases"})
            self.assertEqual(profile["protocol"], "dataset-profile/1")
            phases = [entry["phase"] for entry in profile["phases"]]
            ordinary = [
                "request_admission",
                "host_bindings",
                "snapshot_decode",
                "engine_admission",
                "derivation",
                "output_keys",
                "verification",
            ]
            # A semantic failure closes its actual phase and still encodes a
            # response; later stages must not appear as fictitious zero work.
            self.assertEqual(
                phases, ordinary[: len(phases) - 1] + ["response_encoding"]
            )
            self.assertGreaterEqual(len(phases), 5)
            for text in [profile["total_ns"]] + [
                entry["elapsed_ns"] for entry in profile["phases"]
            ]:
                self.assertIsInstance(text, str)
                self.assertEqual(str(int(text)), text)
                self.assertGreaterEqual(int(text), 0)
            self.assertEqual(
                int(profile["total_ns"]),
                sum(int(entry["elapsed_ns"]) for entry in profile["phases"]),
            )
            profiles.append(profile)
            return table, outcome

        with ExitStack() as stack:
            for name in entrypoints:
                stack.enter_context(patch.object(yamaa_native, name, execute))
            result = unittest.TestResult()
            unittest.defaultTestLoader.loadTestsFromTestCase(case).run(result)
        self.assertTrue(result.wasSuccessful(), result.failures + result.errors)
        self.assertTrue(
            profiles, "the installed corpus must reach actual native execution"
        )

    def test_typed_dataset_truth_and_callback_effects(self):
        """Preserve every existing corpus outcome, handler count, callback trace and error boundary."""
        self.replay(
            installed_datasets.InstalledDatasets,
            ("execute_dataset", "execute_dataset_sources"),
        )
        # The profiler has explicit callback authority. Legacy entrypoints must
        # keep their own no-binding rejection, even within this replay suite.
        self.replay(
            installed_datasets.InstalledDatasetCallbacks,
            ("execute_dataset_functions",),
        )

    def test_project_activation_and_independent_csv(self):
        """Keep activation-before-data, original CSV bytes, cache behavior and fatal callback identity."""
        self.replay(
            installed_project_functions.InstalledProjectFunctions,
            ("execute_dataset_functions",),
        )


if __name__ == "__main__":
    unittest.main()
