/* opensas reference solution for the yamaa benchmark adam-adtte-os.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adrs;
  length STUDYID USUBJID PARAMCD AVALC ANL01FL ADT $1024;
  infile "/app/input/adrs.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. PARAMCD : $char1024. AVALC : $char1024. ANL01FL : $char1024. ADT : $char1024. ASEQ;
run;

data adsl;
  length STUDYID USUBJID RANDDT LSTALVDT $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. RANDDT : $char1024. LSTALVDT : $char1024.;
run;

proc sort data=adrs(where=(PARAMCD='DEATH' and AVALC='Y' and ANL01FL='Y' and ADT ne '')) out=events; by STUDYID USUBJID ADT ASEQ; run;
data firstevent; set events; by STUDYID USUBJID; if first.USUBJID; rename ADT=DEATHDT ASEQ=DEATHSEQ; keep STUDYID USUBJID ADT ASEQ; run;
proc sql; create table joined as select a.*,b.DEATHDT,b.DEATHSEQ from adsl as a left join firstevent as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID; quit;
data result;
  length PARAMCD $8 PARAM $40 STARTDT ADT $10 EVNTDESC CNSDTDSC $40 SRCDOM $4 SRCVAR $16;
  set joined; PARAMCD='OS'; PARAM='Overall Survival'; STARTDT=RANDDT;
  if not missing(DEATHDT) then do;
    ADT=DEATHDT; if ADT<STARTDT then ADT=STARTDT;
    CNSR=0; EVNTDESC='Death'; SRCDOM='ADRS'; SRCVAR='ADT'; SRCSEQ=DEATHSEQ;
  end;
  else if not missing(LSTALVDT) and LSTALVDT>RANDDT then do; ADT=LSTALVDT; CNSR=1; EVNTDESC='Alive'; CNSDTDSC='Alive During Study'; SRCDOM='ADSL'; SRCVAR='LSTALVDT'; end;
  else do; ADT=RANDDT; CNSR=1; EVNTDESC='Randomization'; CNSDTDSC='Randomization'; SRCDOM='ADSL'; SRCVAR='RANDDT'; end;
  if not missing(STARTDT) and not missing(ADT) then AVAL=input(ADT,yymmdd10.)-input(STARTDT,yymmdd10.)+1;
run;

proc sql;
  create table final as select STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC, CNSDTDSC, SRCDOM, SRCVAR, SRCSEQ from result;
quit;
proc export data=final outfile="/app/output/adtte.csv" dbms=csv replace; run;
