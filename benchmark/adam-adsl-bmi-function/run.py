from pathlib import Path

import yamaa
from yamaa.functions import activate_project_functions, function_dispatcher
from yamaa.specification import load_specification

here = Path("spec.yaml").resolve().parent
schema = here.parents[1] / "yaml"
specification = load_specification(here / "spec.yaml", schema).specification
activated = activate_project_functions(specification, here / "python", schema)
adsl = yamaa.yamaa_domain(
    here / "spec.yaml", dispatcher=function_dispatcher(activated)
).output
adsl
