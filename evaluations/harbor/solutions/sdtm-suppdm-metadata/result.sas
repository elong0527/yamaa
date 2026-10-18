/* opensas reference solution for the yamaa benchmark sdtm-suppdm-metadata.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data dm_raw;
  length STUDYID USUBJID RACE1 RACE2 RACEOTH $1024;
  infile "/app/input/dm_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. RACE1 : $char1024.
    RACE2 : $char1024. RACEOTH : $char1024.;
run;

data result;
  set dm_raw;
  length RDOMAIN $2 IDVAR $8 IDVARVAL $40 QNAM $8 QLABEL $40 QVAL $200 QORIG $20
    QEVAL $40;
  RDOMAIN = 'DM';
  QORIG = 'Collected';
  if RACE1 ne '' then do;
    QNAM = 'RACE1'; QLABEL = 'Race 1'; QVAL = RACE1; output;
  end;
  if RACE2 ne '' then do;
    QNAM = 'RACE2'; QLABEL = 'Race 2'; QVAL = RACE2; output;
  end;
  if RACEOTH ne '' then do;
    QNAM = 'RACEOTH'; QLABEL = 'Race, Other'; QVAL = RACEOTH; output;
  end;
run;
proc sql;
  create table final as select STUDYID, RDOMAIN, USUBJID, IDVAR, IDVARVAL, QNAM,
    QLABEL, QVAL, QORIG, QEVAL from result;
quit;
proc export data=final outfile="/app/output/suppdm.csv" dbms=csv replace; run;
