/* opensas reference solution for the yamaa benchmark adam-adsl-bmi.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adsl;
  length STUDYID USUBJID $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. HEIGHTCM WEIGHTKG;
run;

data result;
  set adsl;
  if not missing(HEIGHTCM) and HEIGHTCM ne 0 and not missing(WEIGHTKG) then BMI=WEIGHTKG/(HEIGHTCM/100)**2;
  BMI_FN=BMI;
run;

proc sql;
  create table final as select STUDYID, USUBJID, HEIGHTCM, WEIGHTKG, BMI, BMI_FN from result;
quit;
proc export data=final outfile="/app/output/adsl.csv" dbms=csv replace; run;
