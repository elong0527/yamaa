/* opensas reference solution for the yamaa benchmark adam-adpc-blq-imputation.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data pc;
  length STUDYID DOMAIN USUBJID PCTESTCD PCTEST PCORRES PCORRESU PCSTRESC PCSTRESU PCSTAT PCDTC PCTPT PCTPTREF PCRFTDTC $1024;
  infile "/app/input/pc.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. DOMAIN : $char1024. USUBJID : $char1024. PCSEQ PCTESTCD : $char1024. PCTEST : $char1024. PCORRES : $char1024. PCORRESU : $char1024. PCSTRESC : $char1024. PCSTRESN PCSTRESU : $char1024. PCSTAT : $char1024. PCLLOQ PCDTC : $char1024. PCTPT : $char1024. PCTPTNUM PCTPTREF : $char1024. PCRFTDTC : $char1024.;
run;

/* The quantified samples only: a result below the limit and a sample not
   done have no number, so neither moves the quantified span. */
data quant;
  set pc;
  if not missing(PCSTRESN);
  keep STUDYID USUBJID PCTESTCD PCTPTREF PCTPTNUM;
run;

proc sql;
  create table span as
    select STUDYID, USUBJID, PCTESTCD, PCTPTREF,
           min(PCTPTNUM) as FIRSTQ, max(PCTPTNUM) as LASTQ
    from quant
    group by STUDYID, USUBJID, PCTESTCD, PCTPTREF;
quit;

proc sql;
  create table joined as
    select a.*, s.FIRSTQ, s.LASTQ
    from pc as a left join span as s
      on a.STUDYID = s.STUDYID and a.USUBJID = s.USUBJID
      and a.PCTESTCD = s.PCTESTCD and a.PCTPTREF = s.PCTPTREF;
quit;

/* A quantified result as recorded; a below-limit result is 0 before the
   first or after the last quantified result of its profile (and in a
   profile with no quantified result), and has no value between them. */
data result;
  set joined;
  PARAMCD = PCTESTCD;
  ATPTREF = PCTPTREF;
  ATPT = PCTPT;
  ATPTN = PCTPTNUM;
  AVALU = PCSTRESU;
  if not missing(PCSTRESN) then AVAL = PCSTRESN;
  else if substr(PCSTRESC, 1, 1) = '<' and (missing(FIRSTQ) or PCTPTNUM < FIRSTQ or PCTPTNUM > LASTQ) then AVAL = 0;
  else AVAL = .;
run;

proc sql;
  create table final as select STUDYID, USUBJID, PCSEQ, PARAMCD, ATPTREF, ATPT, ATPTN, AVAL, AVALU from result;
quit;
proc export data=final outfile="/app/output/adpc.csv" dbms=csv replace; run;
