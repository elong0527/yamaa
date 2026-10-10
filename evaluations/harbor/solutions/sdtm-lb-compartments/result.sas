/* SAS language reference solution for the yamaa benchmark sdtm-lb-compartments.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data bx_raw;
  length STUDYID USUBJID COHORT LESRES NLESRES RESU $1024;
  infile "/app/input/bx_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. COHORT : $char1024. LESRES : $char1024. NLESRES : $char1024. RESU : $char1024.;
run;

data prepared; set bx_raw; length DOMAIN $2 LBTESTCD LBTEST LBSPEC LBLOC LBORRES LBORRESU LBSTAT $1024; DOMAIN='LB'; LBTESTCD='IL13'; LBTEST='Interleukin 13'; LBSPEC='SKIN'; LBORRESU=RESU;
if not missing(COHORT) and COHORT ne 'NONAD' then do; LBLOC='LESIONAL'; LBORRES=LESRES; LBSTRESN=input(LESRES,?? best32.); LBSTAT=''; if missing(LESRES) then LBSTAT='NOT DONE'; LOCORD=1; output; end;
LBLOC='NON-LESIONAL'; LBORRES=NLESRES; LBSTRESN=input(NLESRES,?? best32.); LBSTAT=''; if missing(NLESRES) then LBSTAT='NOT DONE'; LOCORD=2; output; run;
proc sort data=prepared; by STUDYID USUBJID LOCORD; run;
data result; set prepared; by STUDYID USUBJID; retain LBSEQ;
  if first.USUBJID then LBSEQ=0;
  LBSEQ+1;
run;

proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBTEST, LBSPEC, LBLOC, LBORRES, LBORRESU, LBSTRESN, LBSTAT from result;
quit;
proc export data=final outfile="/app/output/lb.csv" dbms=csv replace; run;
