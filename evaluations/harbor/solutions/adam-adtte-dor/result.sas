/* opensas reference solution for the yamaa benchmark adam-adtte-dor.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adrs_raw;
  length STUDYID USUBJID ADT AVALC $1024;
  infile "/app/input/adrs_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. ASEQ ADT : $char1024. AVALC : $char1024.;
run;

data adsl;
  length STUDYID USUBJID RESPDT NACTDT $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. RESPDT : $char1024. NACTDT : $char1024.;
run;

data ds;
  length STUDYID USUBJID DSDECOD DSSTDTC $1024;
  infile "/app/input/ds.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. DSSEQ DSDECOD : $char1024. DSSTDTC : $char1024.;
run;

proc sql;
  create table usable as select a.* from adrs_raw as a inner join adsl as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID where a.ADT ne '' and a.AVALC ne 'NE';
  create table progression as select a.* from adrs_raw as a inner join adsl as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID where a.ADT ne '' and a.AVALC='PD' and a.AVALC ne 'NE' and (missing(b.NACTDT) or a.ADT<=b.NACTDT);
  create table deaths as select a.* from ds as a inner join adsl as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID where a.DSDECOD='DEATH' and a.DSSTDTC ne '' and (missing(b.NACTDT) or a.DSSTDTC<=b.NACTDT);
quit;
proc sort data=progression; by STUDYID USUBJID ADT ASEQ; run;
data pd; set progression; by STUDYID USUBJID; if first.USUBJID; keep STUDYID USUBJID ADT ASEQ; rename ADT=PDDT ASEQ=PDSEQ; run;
proc sort data=deaths; by STUDYID USUBJID DSSTDTC DSSEQ; run;
data death; set deaths; by STUDYID USUBJID; if first.USUBJID; keep STUDYID USUBJID DSSTDTC DSSEQ; rename DSSTDTC=DEATHDT DSSEQ=DEATHSEQ; run;
proc sort data=usable; by STUDYID USUBJID descending ADT descending ASEQ; run;
data lastassessment; set usable; by STUDYID USUBJID; if first.USUBJID; keep STUDYID USUBJID ADT ASEQ; rename ADT=LASTDT ASEQ=LASTSEQ; run;
proc sql; create table joined as select a.*,b.PDDT,b.PDSEQ,c.DEATHDT,c.DEATHSEQ,d.LASTDT,d.LASTSEQ from adsl as a
  left join pd as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID left join death as c on a.STUDYID=c.STUDYID and a.USUBJID=c.USUBJID left join lastassessment as d on a.STUDYID=d.STUDYID and a.USUBJID=d.USUBJID; quit;
data result;
  length PARAMCD $8 PARAM $40 STARTDT ADT $10 EVNTDESC CNSDTDSC $40 SRCDOM $4 SRCVAR $16;
  set joined; if not missing(RESPDT);
  PARAMCD='DOR'; PARAM='Duration of Response'; STARTDT=RESPDT;
  if not missing(PDDT) and (missing(DEATHDT) or PDDT<=DEATHDT) then do; ADT=PDDT; CNSR=0; EVNTDESC='DISEASE PROGRESSION'; SRCDOM='ADRS'; SRCVAR='ADT'; SRCSEQ=PDSEQ; end;
  else if not missing(DEATHDT) then do; ADT=DEATHDT; CNSR=0; EVNTDESC='DEATH'; SRCDOM='DS'; SRCVAR='DSSTDTC'; SRCSEQ=DEATHSEQ; end;
  else do;
    CNSR=1; EVNTDESC='CENSORED';
    if not missing(NACTDT) and (missing(LASTDT) or NACTDT<LASTDT) then do; ADT=NACTDT; CNSDTDSC='START OF NEW ANTI-CANCER THERAPY'; SRCDOM='ADSL'; SRCVAR='NACTDT'; end;
    else do; ADT=LASTDT; CNSDTDSC='LAST TUMOUR ASSESSMENT'; SRCDOM='ADRS'; SRCVAR='ADT'; SRCSEQ=LASTSEQ; end;
  end;
  if not missing(ADT) and not missing(STARTDT) then AVAL=input(ADT,yymmdd10.)-input(STARTDT,yymmdd10.)+1;
run;

proc sql;
  create table final as select STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC, CNSDTDSC, SRCDOM, SRCVAR, SRCSEQ from result;
quit;
proc export data=final outfile="/app/output/adtte.csv" dbms=csv replace; run;
