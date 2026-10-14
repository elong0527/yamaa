/* opensas reference solution for the yamaa benchmark adam-adeg-time-matched-concentration.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adeg;
  length STUDYID USUBJID TRTA PARAMCD PARAM ATPT AVALU ABLFL BASETYPE $1024;
  infile "/app/input/adeg_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. TRTA : $char1024. PARAMCD : $char1024. PARAM : $char1024. APERIOD APERDAY ATPT : $char1024. ATPTN AVAL AVALU : $char1024. ABLFL : $char1024. BASE CHG BASETYPE : $char1024.;
  RECNO = _n_;
run;

data adpc;
  length STUDYID USUBJID TRTA ATPT PARAMCD PARAM AVALU LLOQFL $1024;
  infile "/app/input/adpc.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. TRTA : $char1024. APERIOD APERDAY ATPT : $char1024. ATPTN PARAMCD : $char1024. PARAM : $char1024. AVAL AVALU : $char1024. LLOQFL : $char1024.;
run;

/* The parent drug's samples only; metabolite records are never used. */
data parent;
  length P_STUDYID P_USUBJID $1024;
  set adpc;
  if PARAMCD = 'DRGX';
  P_STUDYID = STUDYID; P_USUBJID = USUBJID; P_APERIOD = APERIOD;
  P_APERDAY = APERDAY; P_ATPTN = ATPTN; PKAVAL = AVAL; SAMPLED = 1;
  keep P_STUDYID P_USUBJID P_APERIOD P_APERDAY P_ATPTN PKAVAL SAMPLED;
run;

/* Exact match on subject, period, period day and nominal time point. */
proc sql;
  create table matched as
    select a.*, p.PKAVAL, p.SAMPLED
    from adeg as a left join parent as p
      on a.STUDYID = p.P_STUDYID and a.USUBJID = p.P_USUBJID
      and a.APERIOD = p.P_APERIOD and a.APERDAY = p.P_APERDAY
      and a.ATPTN = p.P_ATPTN
    order by RECNO;
quit;

/* As recorded; nothing imputed, except 0 for placebo without a quantified
   result. */
data final;
  length ECGPCFL CQTANLFL $1;
  set matched;
  if SAMPLED = 1 then ECGPCFL = 'Y';
  else ECGPCFL = '';
  if not missing(PKAVAL) then PKCONC = PKAVAL;
  else if TRTA = 'Placebo' then PKCONC = 0;
  else PKCONC = .;
  if not missing(CHG) and not missing(PKCONC) then CQTANLFL = 'Y';
  else CQTANLFL = '';
run;

proc sql;
  create table out as
    select STUDYID, USUBJID, TRTA, PARAMCD, PARAM, APERIOD, APERDAY, ATPT,
           ATPTN, AVAL, AVALU, ABLFL, BASE, CHG, BASETYPE, ECGPCFL, PKCONC,
           CQTANLFL
    from final;
quit;
proc export data=out outfile="/app/output/adeg.csv" dbms=csv replace; run;
