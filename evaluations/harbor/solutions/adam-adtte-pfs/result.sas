/* opensas reference solution for the yamaa benchmark adam-adtte-pfs.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adsl;
  length STUDYID USUBJID RANDDT NTXSTDT $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. RANDDT : $char1024. NTXSTDT : $char1024.;
run;

data ds;
  length STUDYID USUBJID DSDTC DSDECOD $1024;
  infile "/app/input/ds.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. DSSEQ DSDTC : $char1024. DSDECOD : $char1024.;
run;

data rs;
  length STUDYID USUBJID RSTESTCD RSDTC RSSTRESC ADEQFL $1024;
  infile "/app/input/rs.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. RSSEQ RSTESTCD : $char1024. RSDTC : $char1024. RSSTRESC : $char1024. ADEQFL : $char1024.;
run;

proc sql;
  create table usable as select a.* from rs as a inner join adsl as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID where a.RSDTC ne '' and a.ADEQFL='Y' and a.RSTESTCD='OVRLRESP' and (missing(b.NTXSTDT) or a.RSDTC<=b.NTXSTDT);
  create table progression as select a.* from rs as a inner join adsl as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID where a.RSDTC ne '' and a.RSSTRESC='PD' and a.ADEQFL='Y' and a.RSTESTCD='OVRLRESP' and (missing(b.NTXSTDT) or a.RSDTC<=b.NTXSTDT);
  create table deaths as select a.* from ds as a inner join adsl as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID where a.DSDECOD='DEATH' and a.DSDTC ne '' and (missing(b.NTXSTDT) or a.DSDTC<=b.NTXSTDT);
quit;
proc sort data=progression; by STUDYID USUBJID RSDTC RSSEQ; run;
data pd; set progression; by STUDYID USUBJID; if first.USUBJID; keep STUDYID USUBJID RSDTC RSSEQ; rename RSDTC=PDDT RSSEQ=PDSEQ; run;
proc sort data=deaths; by STUDYID USUBJID DSDTC DSSEQ; run;
data death; set deaths; by STUDYID USUBJID; if first.USUBJID; keep STUDYID USUBJID DSDTC DSSEQ; rename DSDTC=DEATHDT DSSEQ=DEATHSEQ; run;
proc sort data=usable; by STUDYID USUBJID descending RSDTC descending RSSEQ; run;
data lastassessment; set usable; by STUDYID USUBJID; if first.USUBJID; keep STUDYID USUBJID RSDTC RSSEQ; rename RSDTC=LASTDT RSSEQ=LASTSEQ; run;
proc sql; create table joined as select a.*,b.PDDT,b.PDSEQ,c.DEATHDT,c.DEATHSEQ,d.LASTDT,d.LASTSEQ from adsl as a
  left join pd as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID left join death as c on a.STUDYID=c.STUDYID and a.USUBJID=c.USUBJID left join lastassessment as d on a.STUDYID=d.STUDYID and a.USUBJID=d.USUBJID; quit;
data result;
  length PARAMCD $8 PARAM $40 STARTDT ADT $10 EVNTDESC CNSDTDSC $40 SRCDOM $4 SRCVAR $16;
  set joined;
  PARAMCD='PFS'; PARAM='Progression-Free Survival'; STARTDT=RANDDT;
  if not missing(PDDT) and (missing(DEATHDT) or PDDT<=DEATHDT) then do; ADT=PDDT; CNSR=0; EVNTDESC='DISEASE PROGRESSION'; SRCDOM='RS'; SRCVAR='RSDTC'; SRCSEQ=PDSEQ; end;
  else if not missing(DEATHDT) then do; ADT=DEATHDT; CNSR=0; EVNTDESC='DEATH'; SRCDOM='DS'; SRCVAR='DSDTC'; SRCSEQ=DEATHSEQ; end;
  else do;
    CNSR=1; EVNTDESC='CENSORED';
    if not missing(LASTDT) then do; ADT=LASTDT; SRCDOM='RS'; SRCVAR='RSDTC'; SRCSEQ=LASTSEQ; end; else ADT=RANDDT;
  end;
  if not missing(ADT) and not missing(STARTDT) then AVAL=input(ADT,yymmdd10.)-input(STARTDT,yymmdd10.)+1;
run;

proc sql;
  create table final as select STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC, SRCDOM, SRCVAR, SRCSEQ from result;
quit;
proc export data=final outfile="/app/output/adtte.csv" dbms=csv replace; run;
