/* SAS language reference solution for the yamaa benchmark adam-adtte-pro-deterioration.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data adsl;
  length STUDYID USUBJID RANDDT DTHDT $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. RANDDT : $char1024. DTHDT : $char1024.;
run;

data ds;
  length STUDYID USUBJID DSDECOD DSDTC $1024;
  infile "/app/input/ds.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. DSDECOD : $char1024. DSDTC : $char1024. DSSEQ;
run;

data qs;
  length STUDYID USUBJID AVISIT ADT $1024;
  infile "/app/input/qs.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. QSSEQ AVISIT : $char1024. ADT : $char1024. AVAL;
run;

proc sort data=qs(where=(ADT ne '')) out=ordered; by STUDYID USUBJID ADT QSSEQ; run;
data deteriorations;
  length BASEDT $10;
  retain BASEDT BASEVAL;
  set ordered; by STUDYID USUBJID;
  if first.USUBJID then do; BASEDT=ADT; BASEVAL=AVAL; end;
  if ADT>BASEDT and not missing(AVAL) and not missing(BASEVAL) and AVAL<=BASEVAL-10;
run;
data deterioration; set deteriorations; by STUDYID USUBJID; if first.USUBJID; keep STUDYID USUBJID ADT QSSEQ; rename ADT=DETERDT QSSEQ=DETERSEQ; run;
proc sort data=ds(where=(DSDECOD in ('DISEASE PROGRESSION','STUDY DISCONTINUATION','WITHDRAWAL OF CONSENT'))) out=reasons; by STUDYID USUBJID DSDTC DSSEQ; run;
data reason; set reasons; by STUDYID USUBJID; if first.USUBJID; keep STUDYID USUBJID DSDTC DSDECOD; rename DSDTC=REASONDT DSDECOD=REASON; run;
proc sql; create table assessment_reasons as select a.*,b.REASONDT from ordered as a left join reason as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID; quit;
data candidates; set assessment_reasons; if missing(REASONDT) or ADT<=REASONDT; run;
proc sort data=candidates; by STUDYID USUBJID descending ADT descending QSSEQ; run;
data lastassessment; set candidates; by STUDYID USUBJID; if first.USUBJID; keep STUDYID USUBJID ADT QSSEQ; rename ADT=LASTDT QSSEQ=LASTSEQ; run;
proc sql; create table joined as select a.*,b.DETERDT,b.DETERSEQ,c.REASONDT,c.REASON,d.LASTDT,d.LASTSEQ from adsl as a
  left join deterioration as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID left join reason as c on a.STUDYID=c.STUDYID and a.USUBJID=c.USUBJID left join lastassessment as d on a.STUDYID=d.STUDYID and a.USUBJID=d.USUBJID; quit;
data result;
  length PARAMCD $8 PARAM $60 STARTDT ADT $10 EVNTDESC CNSDTDSC $40 SRCDOM $4 SRCVAR $16;
  set joined; PARAMCD='TTDGHS'; PARAM='Time to Deterioration in Global Health Status'; STARTDT=RANDDT;
  if not missing(DETERDT) and (missing(DTHDT) or DETERDT<=DTHDT) and (missing(REASONDT) or DETERDT<=REASONDT) then do; ADT=DETERDT; CNSR=0; EVNTDESC='PRO DETERIORATION'; SRCDOM='QS'; SRCVAR='ADT'; SRCSEQ=DETERSEQ; end;
  else if not missing(DTHDT) and (missing(REASONDT) or DTHDT<=REASONDT) then do; ADT=DTHDT; CNSR=0; EVNTDESC='DEATH'; SRCDOM='ADSL'; SRCVAR='DTHDT'; end;
  else do;
    CNSR=1; EVNTDESC='CENSORED'; CNSDTDSC=coalescec(REASON,'STUDY COMPLETION');
    if not missing(LASTDT) then do; ADT=LASTDT; SRCDOM='QS'; SRCVAR='ADT'; SRCSEQ=LASTSEQ; end;
    else do; ADT=RANDDT; SRCDOM='ADSL'; SRCVAR='RANDDT'; end;
  end;
  if not missing(STARTDT) and not missing(ADT) then AVAL=intck('month',input(STARTDT,yymmdd10.),input(ADT,yymmdd10.),'continuous');
run;

proc sql;
  create table final as select STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC, CNSDTDSC, SRCDOM, SRCVAR, SRCSEQ from result;
quit;
proc export data=final outfile="/app/output/adtte.csv" dbms=csv replace; run;
