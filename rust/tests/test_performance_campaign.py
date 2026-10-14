"""Same-machine receipts retain failures without admitting missing baselines."""

import copy
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

SPEC = importlib.util.spec_from_file_location(
    "performance_campaign",
    Path(__file__).resolve().parents[1] / "tools/performance_campaign.py",
)
campaign = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(campaign)


class ActualCampaign(unittest.TestCase):
    def test_same_machine_receipt_keeps_sampling_scope_and_original_refusals(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            receipt = root / "campaign.json"
            original = campaign.create(receipt)
            self.assertTrue(original["measurement_machine"]["node"])
            identities = []
            for role in ("candidate", "baseline", "producer-root-build-baseline"):
                folder = root / role
                folder.mkdir()
                collection = {
                    "protocol": "public-performance/1",
                    "source_revision": role,
                    "provenance": {"system": original["measurement_machine"]["system"]},
                    "purpose": "capacity-frontier",
                    "accepted": False,
                    "measurement_scope": "producer-root-build"
                    if role.startswith("producer")
                    else "full-lifecycle",
                    "sampling": {"cold": 10, "warm": 30, "warmups": 3},
                    "cases": [{"accepted": False, "failure": "original refusal"}],
                }
                campaign.write(folder / "collection.json", collection)
                attached = campaign.attach(receipt, folder, role)
                identities.append(
                    (attached["campaign_id"], attached["measurement_machine"])
                )
                retained = campaign.read(folder / "collection.json")
                retained.pop("measurement_campaign")
                self.assertEqual(retained, collection)
                campaign.attach(receipt, folder, role)
            self.assertEqual(identities, [identities[0]] * 3)
            self.assertEqual(len(campaign.read(receipt)["collections"]), 3)
            with self.assertRaises(FileExistsError):
                campaign.create(receipt)

    def test_different_machine_cannot_relabel_an_original_collection(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            receipt = root / "campaign.json"
            actual = campaign.create(receipt)
            folder = root / "candidate"
            folder.mkdir()
            campaign.write(folder / "collection.json", {"original": "bytes"})
            before = (folder / "collection.json").read_bytes()
            other = copy.deepcopy(actual["measurement_machine"])
            other["node"] += "-other"
            with (
                mock.patch.object(campaign, "machine", return_value=other),
                self.assertRaisesRegex(ValueError, "another actual machine"),
            ):
                campaign.attach(receipt, folder, "candidate")
            self.assertEqual((folder / "collection.json").read_bytes(), before)

    def test_wrong_system_or_protocol_never_attaches(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            receipt = root / "campaign.json"
            actual = campaign.create(receipt)
            folder = root / "candidate"
            folder.mkdir()
            for wrong in (
                {
                    "protocol": "other",
                    "provenance": {"system": actual["measurement_machine"]["system"]},
                },
                {
                    "protocol": "public-performance/1",
                    "provenance": {"system": "OtherOS"},
                },
            ):
                campaign.write(folder / "collection.json", wrong)
                with self.assertRaisesRegex(ValueError, "system/protocol"):
                    campaign.attach(receipt, folder, "candidate")
                self.assertNotIn(
                    "measurement_campaign",
                    json.loads((folder / "collection.json").read_text()),
                )
