"""Independent controls for actual owner phase admission and raw transport."""
import copy
import gzip
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

TOOLS=Path(__file__).resolve().parents[1]/"tools"
SPEC=importlib.util.spec_from_file_location("actual_measurements",TOOLS/"application_measurement_evidence.py")
phases=importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(phases)


def evidence():
    names=["preparation","build_service","input","execution_admission","execution_derivation","output_keys","verification","output_preparation"]
    return {"protocol":"application-measurements/1","types":{"offsets":"uint64 nanoseconds since owner origin","function_counts":"uint64 actual calls","resource_counts":"usize; required null preserves original overflow/absence"},"clock":{"provider":"std::time::Instant","unit":"nanoseconds","origin":"owner-local monotonic origin immediately before original application entry"},"admitted":True,"refusal":None,"original_execution_finished":True,"build_accepted":True,"retention_limits":{"nodes":64,"identity_bytes":1_048_576,"stage_records":1024,"save_records":16},"nodes":[],"stages":[{"node":None,"phase":name,"entered_ns":2**53+index,"completed_ns":2**53+index+1,"outcome":"succeeded"} for index,name in enumerate(names)],"function_calls":[{"node":None,"stage":name,"entered":0,"succeeded":0,"failed":0} for name in names if name in {"execution_admission","execution_derivation","output_keys","verification"}],"classified_refusals":[],"save_attempts":0,"saves":[]}


class ActualEvidence(unittest.TestCase):
    def test_large_report_raw_bytes_and_original_uint64_are_retained_separately(self):
        report=json.dumps({"tables":[{"rows":["é"*600000]}],"nested":{"measurements":{"original":True}}},ensure_ascii=False,separators=(",",":"))
        original=evidence()
        envelope=phases.PREFIX+report+phases.DELIMITER+json.dumps(original,separators=(",",":"))+"}"
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            for suffix in ("", ".gz"):
                target=root/("report.json"+suffix); metrics=root/("metrics.json"+suffix)
                phases.retain_envelope(envelope,target,metrics)
                read=lambda path: gzip.decompress(path.read_bytes()) if suffix else path.read_bytes()
                self.assertEqual(read(target),report.encode())
                self.assertEqual(json.loads(read(metrics)),original)
                self.assertEqual(json.loads(read(metrics))["stages"][0]["entered_ns"],2**53)

    def test_incomplete_or_refused_evidence_never_admits_as_timing(self):
        original=evidence()
        self.assertEqual(len(phases.validate(original,0)),8)
        for key,value in (("admitted",False),("build_accepted",False),("original_execution_finished",False),("refusal","stage_event_capacity")):
            changed=copy.deepcopy(original); changed[key]=value
            with self.assertRaisesRegex(ValueError,"refused or incomplete"): phases.validate(changed,0)
        for key,value in (("entered_ns",True),("completed_ns",None),("outcome","failed")):
            changed=copy.deepcopy(original);changed["stages"][0][key]=value
            with self.assertRaises(ValueError): phases.validate(changed,0)

    def test_missing_phase_unmatched_function_and_failed_save_are_rejected(self):
        value=evidence();value["stages"].pop()
        with self.assertRaisesRegex(ValueError,"phases are missing"):phases.validate(value,0)
        value=evidence();value["function_calls"]=[{"node":None,"stage":"activation","entered":1,"succeeded":0,"failed":0}]
        with self.assertRaisesRegex(ValueError,"return ledger is incomplete"):phases.validate(value,0)
        value=evidence();value["save_attempts"]=1;value["saves"]=[{"attempt":1,"entered_ns":10,"completed_ns":11,"outcome":"refused"}]
        with self.assertRaisesRegex(ValueError,"Save did not succeed"):phases.validate(value,1)

    def test_reading_later_cannot_invent_original_phase_events(self):
        case={"family":"lookup"};row={"measurements":{name:name for name in ("build","save_1","save_2")}}
        values={}
        for name,count in (("build",0),("save_1",1),("save_2",2)):
            value=evidence();value["save_attempts"]=count;value["saves"]=[{"attempt":i+1,"entered_ns":2**53+100+i*2,"completed_ns":2**53+101+i*2,"outcome":"succeeded"} for i in range(count)];values[name]=value
        phases.validate_repetition(case,Path("/retained"),row,lambda path: values[path.name])
        values["save_2"]["stages"][0]["entered_ns"]-=1
        with self.assertRaisesRegex(ValueError,"fabricated original events"):phases.validate_repetition(case,Path("/retained"),row,lambda path: values[path.name])
