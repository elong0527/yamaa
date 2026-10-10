/* opensas reference solution for the yamaa benchmark adam-adfa-fever.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data vs;
  length STUDYID USUBJID VSTESTCD VSCAT VSSTRESU VSDTC $1024;
  infile "/app/input/vs.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. VSSEQ VSTESTCD : $char1024. VSCAT : $char1024. VSSTRESN VSSTRESU : $char1024. VSDTC : $char1024.;
run;

proc sort data=vs(where=(VSTESTCD='TEMP' and VSCAT='REACTOGENICITY')) out=ordered; by STUDYID USUBJID VSDTC VSSEQ; run;
data result;
  length DOMAIN $4 PARAMCD $16 PARAM $40 AVALC $1 ADT $10 SRCDOM $2 SRCVAR $16;
  set ordered; by STUDYID USUBJID;
  if first.USUBJID then ASEQ=0; ASEQ+1;
  DOMAIN='ADFA'; PARAMCD='FEVER'; PARAM='Fever Occurrence'; ADT=substr(VSDTC,1,10);
  SRCDOM='VS'; SRCVAR='VSSTRESN'; SRCSEQ=VSSEQ;
  if not missing(VSSTRESN) and VSSTRESU='C' then do;
    if VSSTRESN>=38 then do; AVAL=1; AVALC='Y'; end;
    else do; AVAL=0; AVALC='N'; end;
  end;
run;

proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, ASEQ, PARAMCD, PARAM, AVAL, AVALC, ADT, SRCDOM, SRCVAR, SRCSEQ from result;
quit;
proc export data=final outfile="/app/output/adfa.csv" dbms=csv replace; run;
