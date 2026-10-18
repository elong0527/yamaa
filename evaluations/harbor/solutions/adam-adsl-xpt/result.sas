/* opensas reference solution for the yamaa benchmark adam-adsl-xpt.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adsl_raw;
  length STUDYID USUBJID TRT01A TRTSDTC $1024;
  infile "/app/input/adsl_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. TRT01A : $char1024. AGE HEIGHTBL
    TRTSDTC : $char1024.;
run;

data result;
  set adsl_raw;
  length TRTSDTM $19 TRTSDT $10;
  /* A collected time without seconds names second zero. */
  if TRTSDTC ne '' then do;
    if length(TRTSDTC) = 16 then TRTSDTM = cats(TRTSDTC, ':00');
    else TRTSDTM = TRTSDTC;
    TRTSDT = substr(TRTSDTC, 1, 10);
  end;
run;
proc sql;
  create table final as select STUDYID, USUBJID, TRT01A, AGE, HEIGHTBL, TRTSDT, TRTSDTM
    from result;
quit;
proc export data=final outfile="/app/output/adsl.csv" dbms=csv replace; run;
