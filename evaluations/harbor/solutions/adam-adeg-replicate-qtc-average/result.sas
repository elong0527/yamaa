/* opensas reference solution for the yamaa benchmark adam-adeg-replicate-qtc-average.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data eg;
  length STUDYID DOMAIN USUBJID EGTESTCD EGTEST EGSTAT EGSTRESU EGTPT EGDTC $1024;
  infile "/app/input/eg.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. DOMAIN : $char1024. USUBJID : $char1024. EGSEQ
        EGTESTCD : $char1024. EGTEST : $char1024. EGREPNUM EGSTAT : $char1024.
        EGSTRESN EGSTRESU : $char1024. EGTPT : $char1024. EGTPTNUM
        EGDTC : $char1024.;
run;

/* The QT records, and the RR records under their own column names. */
data qt;
  set eg;
  if EGTESTCD='QTAG';
  QT=EGSTRESN;
  keep STUDYID USUBJID EGTPT EGTPTNUM EGREPNUM QT;
run;
data rr;
  length R_STUDYID R_USUBJID R_TPT $1024;
  set eg;
  if EGTESTCD='RRAG';
  R_STUDYID=STUDYID; R_USUBJID=USUBJID; R_TPT=EGTPT;
  R_TPTNUM=EGTPTNUM; R_REPNUM=EGREPNUM;
  RR=EGSTRESN;
  keep R_STUDYID R_USUBJID R_TPT R_TPTNUM R_REPNUM RR;
run;

/* One ECG is one replicate number at a time point: its QT pairs with the
   RR of the same replicate number, never by collection time. */
proc sql;
  create table pairs as
    select a.STUDYID, a.USUBJID, a.EGTPT, a.EGTPTNUM, a.QT, b.RR
    from qt as a left join rr as b
      on a.STUDYID=b.R_STUDYID and a.USUBJID=b.R_USUBJID
         and a.EGTPTNUM=b.R_TPTNUM and a.EGREPNUM=b.R_REPNUM;
quit;

/* Fridericia's correction of each ECG: QT over the cube root of RR in
   seconds; an ECG without both results has no corrected value. */
data pairs;
  set pairs;
  if not missing(QT) and not missing(RR) and RR ne 0 then QTCF=QT/(RR/1000)**(1/3);
  else QTCF=.;
run;

/* The mean over the ECGs that have a value; no value, no record. */
proc sql;
  create table qtavg as
    select STUDYID, USUBJID, EGTPT, EGTPTNUM, mean(QT) as AVAL
    from qt where QT is not missing
    group by STUDYID, USUBJID, EGTPTNUM, EGTPT;
  create table rravg as
    select R_STUDYID as STUDYID, R_USUBJID as USUBJID, R_TPT as EGTPT,
           R_TPTNUM as EGTPTNUM, mean(RR) as AVAL
    from rr where RR is not missing
    group by R_STUDYID, R_USUBJID, R_TPTNUM, R_TPT;
  create table qtcfavg as
    select STUDYID, USUBJID, EGTPT, EGTPTNUM, mean(QTCF) as AVAL
    from pairs where QTCF is not missing
    group by STUDYID, USUBJID, EGTPTNUM, EGTPT;
quit;

data result;
  length PARAMCD PARAM ATPT DTYPE $1024;
  set qtavg(in=inqt) rravg(in=inrr) qtcfavg(in=inqtcf);
  if inqt then do; PARAMCD='QTAG'; PARAM='QT Interval, Aggregate (msec)'; end;
  else if inrr then do; PARAMCD='RRAG'; PARAM='RR Interval, Aggregate (msec)'; end;
  else do; PARAMCD='QTCF'; PARAM='QTcF Interval, Aggregate (msec)'; end;
  ATPT=EGTPT; ATPTN=EGTPTNUM; DTYPE='AVERAGE';
run;

proc sort data=result; by STUDYID USUBJID PARAMCD ATPTN; run;

proc sql;
  create table final as
    select STUDYID, USUBJID, PARAMCD, PARAM, ATPT, ATPTN, DTYPE, AVAL
    from result;
quit;
proc export data=final outfile="/app/output/adeg.csv" dbms=csv replace; run;
