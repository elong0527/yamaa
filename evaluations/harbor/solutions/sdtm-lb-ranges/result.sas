/* SAS language reference solution for the yamaa benchmark sdtm-lb-ranges.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data dm;
  length STUDYID USUBJID SEX $1024;
  infile "/app/input/dm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. SEX : $char1024.;
run;

data lb_raw;
  length STUDYID USUBJID LBTESTCD $1024;
  infile "/app/input/lb_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. LBSEQ LBTESTCD : $char1024. LBSTRESN;
run;

data lbrange;
  length LBTESTCD SEX UNIT $1024;
  infile "/app/input/lbrange.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input LBTESTCD : $char1024. SEX : $char1024. UNIT : $char1024. NRLO NRHI;
run;

proc sql;
create table joined as select a.*,c.UNIT as LBSTRESU,c.NRLO as LBSTNRLO,c.NRHI as LBSTNRHI from lb_raw a left join dm b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID left join lbrange c on a.LBTESTCD=c.LBTESTCD and b.SEX=c.SEX; quit;
data result; set joined; length DOMAIN $2 LBNRIND $8; DOMAIN='LB'; LBNRIND=''; if not missing(LBSTRESN) and not missing(LBSTNRLO) and not missing(LBSTNRHI) then do; if LBSTRESN<LBSTNRLO then LBNRIND='LOW'; else if LBSTRESN>LBSTNRHI then LBNRIND='HIGH'; else LBNRIND='NORMAL'; end; run;
proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBSTRESN, LBSTRESU, LBSTNRLO, LBSTNRHI, LBNRIND from result;
quit;
proc export data=final outfile="/app/output/lb.csv" dbms=csv replace; run;
