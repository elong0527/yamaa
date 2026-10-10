/* opensas reference solution for the yamaa benchmark adam-advs-percentiles.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data advs_raw;
  length STUDYID USUBJID AVISIT PARAMCD SEX $1024;
  infile "/app/input/advs_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AVISIT : $char1024. PARAMCD : $char1024. AVAL AGE SEX : $char1024.;
run;

data lms_ref;
  length PARAMCD SEX $1024;
  infile "/app/input/lms_ref.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input PARAMCD : $char1024. SEX : $char1024. AGE L M S;
run;

proc sql;
  create table joined as select a.*,b.L,b.M,b.S from advs_raw as a left join lms_ref as b on a.PARAMCD=b.PARAMCD and a.SEX=b.SEX and a.AGE=b.AGE;
quit;
data result;
  length PARAM $40;
  set joined;
  value=AVAL; AVAL=.;
  if not missing(value) and not missing(M) and not missing(L) and not missing(S) then do;
    if L=0 then z=log(value/M)/S; else z=((value/M)**L-1)/(L*S);
    AVAL=100*probnorm(z);
  end;
  if PARAMCD='BMI' then do; PARAMCD='BMIPCTL'; PARAM='BMI-for-Age Percentile'; end;
  else if PARAMCD='WEIGHT' then do; PARAMCD='WGTPCTL'; PARAM='Weight-for-Age Percentile'; end;
run;

proc sql;
  create table final as select STUDYID, USUBJID, AVISIT, PARAMCD, PARAM, AVAL from result;
quit;
proc export data=final outfile="/app/output/advs.csv" dbms=csv replace; run;
