/* opensas reference solution for the yamaa benchmark sdtm-dm-arms.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data ex;
  length STUDYID USUBJID EXTRT $1024;
  infile "/app/input/ex.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. EXTRT : $char1024.;
run;

data rand;
  length STUDYID USUBJID RANDCD RAND SCRNFL $1024;
  infile "/app/input/rand.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. RANDCD : $char1024. RAND : $char1024. SCRNFL : $char1024.;
run;

/* Keep the first nonmissing collected exposure per subject. */
data nonmissing_ex; set ex; if not missing(EXTRT); EXORDER=_N_; run;
proc sort data=nonmissing_ex; by STUDYID USUBJID EXORDER; run;
data exposure; set nonmissing_ex; by STUDYID USUBJID; if first.USUBJID; run;
proc sql; create table joined as select a.*,b.EXTRT from rand a left join exposure b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID; quit;
data result; set joined; length DOMAIN $2 ARMCD ARM ACTARMCD ACTARM ARMNRS ACTARMUD $1024; DOMAIN='DM'; ARMCD=RANDCD; ARM=RAND;
if EXTRT='PLACEBO' then do; ACTARMCD='PBO'; ACTARM='Placebo'; end;
else if EXTRT='VITAMIN D3' then do; ACTARMCD='TRT'; ACTARM='Vitamin D3'; end;
else if not missing(EXTRT) then ACTARMUD=EXTRT;
if missing(EXTRT) then do; if not missing(ARMCD) then ARMNRS='NOT TREATED'; else if SCRNFL='Y' then ARMNRS='SCREEN FAILURE'; else ARMNRS='NOT ASSIGNED'; end; run;
proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, ARMCD, ARM, ACTARMCD, ACTARM, ARMNRS, ACTARMUD from result;
quit;
proc export data=final outfile="/app/output/dm.csv" dbms=csv replace; run;
