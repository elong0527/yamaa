/* SAS language reference solution for the yamaa benchmark adam-adsl-randomization.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

/* openSAS v0.6.6 has no Parquet reader. The runner must stage dm.parquet
   as dm.csv without deriving any values, preserving ISO dates/timestamps. */
data dm;
  length STUDYID USUBJID $1024 RANDDT RFSTDTC $10 RANDDTTM $32;
  infile '/app/input/dm.csv' dsd dlm=',' firstobs=2 truncover;
  input STUDYID : $char1024. USUBJID : $char1024. RFSTDTC : $char10. RANDDT : $char10. RANDDTTM : $char32.;
run;
data result;
  length RANDDTC $19;
  set dm;
  if not missing(RANDDT) and not missing(RFSTDTC) then do;
    RANDDY=input(RANDDT,yymmdd10.)-input(RFSTDTC,yymmdd10.);
    if RANDDY>=0 then RANDDY=RANDDY+1;
  end;
  if not missing(RANDDTTM) then do;
    RANDDTC=translate(substr(RANDDTTM,1,19),'T',' ');
    if lengthn(RANDDTTM)=16 then RANDDTC=cats(RANDDTTM,':00');
  end;
run;

proc sql;
  create table final as select STUDYID, USUBJID, RANDDT, RANDDY, RANDDTC from result;
quit;
proc export data=final outfile="/app/output/adsl.csv" dbms=csv replace; run;
