/* opensas reference solution for the yamaa benchmark adam-adlb-bds.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adsl;
  length STUDYID USUBJID TRTSDT TRT01A $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. TRTSDT : $char1024. TRT01A : $char1024.;
run;

data lb;
  length STUDYID USUBJID LBTESTCD LBTEST LBDTC LBSTRESU $1024;
  infile "/app/input/lb.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. LBTESTCD : $char1024. LBTEST : $char1024. LBDTC : $char1024. LBSTRESN LBSTRESU : $char1024.;
run;

data parameters;
  length PARAMCD $16 PARAM $40 ADT $10 AVALU $1024;
  set lb;
  if LBTESTCD in ('ALT','AST') and not missing(LBSTRESN);
  ADT=LBDTC; PARAMCD=LBTESTCD; AVAL=LBSTRESN; AVALU=LBSTRESU;
  if LBTESTCD='ALT' then PARAM='Alanine Aminotransferase'; else PARAM='Aspartate Aminotransferase';
  output;
  if LBTESTCD='ALT' then do; PARAMCD='ALTSI'; PARAM='Alanine Aminotransferase (SI)'; AVAL=LBSTRESN*0.0167; AVALU='ukat/L'; output; end;
run;
proc sql; create table joined as select a.*,b.TRTSDT,b.TRT01A from parameters as a left join adsl as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID; quit;
proc sort data=joined(where=(ADT ne '' and TRTSDT ne '' and ADT<=TRTSDT)) out=candidates; by STUDYID USUBJID PARAMCD descending ADT; run;
data baseline; set candidates; by STUDYID USUBJID PARAMCD; if first.PARAMCD; BASE=AVAL; keep STUDYID USUBJID PARAMCD ADT BASE; rename ADT=BASEDT; run;
proc sql; create table combined as select a.*,b.BASE,b.BASEDT from joined as a left join baseline as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.PARAMCD=b.PARAMCD; quit;
proc sort data=combined; by STUDYID USUBJID PARAMCD ADT; run;
data result;
  length ABLFL $1;
  set combined; by STUDYID USUBJID;
  if first.USUBJID then ASEQ=0; ASEQ+1;
  if ADT=BASEDT and not missing(BASEDT) then ABLFL='Y';
  CHG=AVAL-BASE;
  if not missing(BASE) and BASE ne 0 then PCHG=100*CHG/BASE;
run;

proc sql;
  create table final as select STUDYID, USUBJID, PARAMCD, PARAM, ADT, TRTSDT, TRT01A, AVAL, AVALU, ABLFL, BASE, CHG, PCHG, ASEQ from result;
quit;
proc export data=final outfile="/app/output/adlb.csv" dbms=csv replace; run;
