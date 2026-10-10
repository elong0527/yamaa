/* opensas reference solution for the yamaa benchmark adam-adlb-shift-criteria.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adlb_raw;
  length STUDYID USUBJID PARAMCD PARAM AVISIT ABLFL $1024;
  infile "/app/input/adlb_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. PARAMCD : $char1024. PARAM : $char1024. ASEQ AVISIT : $char1024. AVAL ANRLO ANRHI ABLFL : $char1024.;
run;

data marked;
  length ANRIND $6;
  set adlb_raw;
  if not missing(AVAL) and not missing(ANRLO) and not missing(ANRHI) then do;
    if AVAL<ANRLO then ANRIND='LOW'; else if AVAL>ANRHI then ANRIND='HIGH'; else ANRIND='NORMAL';
  end;
run;
proc sql; create table combined as select a.*,b.AVAL as BASE,b.ANRIND as BNRIND from marked as a left join marked as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.PARAMCD=b.PARAMCD and b.ABLFL='Y'; quit;
data result;
  length SHIFT1 $20 CRIT1 $30 CRIT2 $30 CRIT1FL CRIT2FL $1;
  set combined;
  if not missing(ANRIND) and not missing(BNRIND) then SHIFT1=catx(' to ',BNRIND,ANRIND);
  if not missing(AVAL) and not missing(BASE) and BASE ne 0 then R2BASE=AVAL/BASE;
  if not missing(AVAL) and not missing(ANRHI) then do;
    CRIT1='Result greater than 3 x ULN'; CRIT1FL='N'; if AVAL>3*ANRHI then CRIT1FL='Y';
  end;
  if not missing(AVAL) and not missing(ANRLO) then do;
    CRIT2='Result less than LLN'; CRIT2FL='N'; if AVAL<ANRLO then CRIT2FL='Y';
  end;
run;

proc sql;
  create table final as select STUDYID, USUBJID, PARAMCD, PARAM, ASEQ, AVISIT, AVAL, ANRLO, ANRHI, ANRIND, ABLFL, BASE, BNRIND, SHIFT1, R2BASE, CRIT1, CRIT1FL, CRIT2, CRIT2FL from result;
quit;
proc export data=final outfile="/app/output/adlb.csv" dbms=csv replace; run;
