/* opensas reference solution for the yamaa benchmark adam-adae-analysis-time.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data ae;
  length STUDYID USUBJID AETERM AESTDTC $1024;
  infile "/app/input/ae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AETERM : $char1024. AESTDTC : $char1024.;
run;

/* Complete each collected start by its length: the missing parts of the time
   take their earliest values, and the flag names the coarsest part supplied.
   A start without its day, or no start, gives no analysis time. The time of
   day counts the seconds since midnight. */
data result;
  length ASTDTM $19 ASTDT $10 ASTTMF $1;
  set ae;
  ASTDTM=''; ASTDT=''; ASTTMF=''; ASTTM=.;
  ls=lengthn(AESTDTC);
  if ls=19 then ASTDTM=AESTDTC;
  else if ls=16 then do; ASTDTM=cats(AESTDTC, ':00'); ASTTMF='S'; end;
  else if ls=13 then do; ASTDTM=cats(AESTDTC, ':00:00'); ASTTMF='M'; end;
  else if ls=10 then do; ASTDTM=cats(AESTDTC, 'T00:00:00'); ASTTMF='H'; end;
  if ASTDTM ne '' then do;
    ASTDT=substr(ASTDTM, 1, 10);
    ASTTM=input(substr(ASTDTM, 12, 2), 2.)*3600 + input(substr(ASTDTM, 15, 2), 2.)*60
      + input(substr(ASTDTM, 18, 2), 2.);
  end;
run;

proc sql;
  create table final as select STUDYID, USUBJID, AESEQ, AETERM, ASTDTM, ASTDT, ASTTM, ASTTMF
    from result;
quit;
proc export data=final outfile="/app/output/adae.csv" dbms=csv replace; run;
