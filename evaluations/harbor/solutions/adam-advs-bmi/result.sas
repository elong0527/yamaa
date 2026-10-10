/* SAS language reference solution for the yamaa benchmark adam-advs-bmi.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data adsl;
  length STUDYID USUBJID $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. HEIGHTBL;
run;

data vs;
  length STUDYID USUBJID VSTESTCD VSTEST VISIT $1024;
  infile "/app/input/vs.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. VSSEQ VSTESTCD : $char1024. VSTEST : $char1024. VSSTRESN VISIT : $char1024.;
run;

proc sql; create table joined as select a.*,b.HEIGHTBL from vs as a left join adsl as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID; quit;
data result;
  length AVISIT $1024 PARAMCD $16 PARAM $1024;
  set joined;
  AVISIT=VISIT; PARAMCD=VSTESTCD; PARAM=VSTEST; AVAL=VSSTRESN; output;
  if VSTESTCD='WEIGHT' and not missing(VSSTRESN) then do;
    PARAMCD='BMI'; PARAM='Body Mass Index (kg/m^2)'; AVAL=.;
    if not missing(HEIGHTBL) and HEIGHTBL ne 0 then AVAL=VSSTRESN/(HEIGHTBL/100)**2;
    output;
  end;
run;

proc sql;
  create table final as select STUDYID, USUBJID, AVISIT, PARAMCD, PARAM, AVAL from result;
quit;
proc export data=final outfile="/app/output/advs.csv" dbms=csv replace; run;
