/* opensas reference solution for the yamaa benchmark adam-adcm-related-events.
   Reads the benchmark inputs and derives the requested outputs.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data ae;
  length STUDYID USUBJID $40 AETERM $200;
  infile "/app/input/ae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char40. USUBJID : $char40. AESEQ AETERM : $char200.;
run;

data cm;
  length STUDYID USUBJID $40 CMTRT $200;
  infile "/app/input/cm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char40. USUBJID : $char40. CMSEQ CMTRT : $char200.;
run;

data relrec;
  length STUDYID RDOMAIN USUBJID IDVAR IDVARVAL RELTYPE RELID $40;
  infile "/app/input/relrec.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char40. RDOMAIN : $char40. USUBJID : $char40.
        IDVAR : $char40. IDVARVAL : $char40. RELTYPE : $char40.
        RELID : $char40.;
run;

/* The medications and the adverse events each relationship names, with
   their numeric sequence numbers. Function calls in join predicates were
   rejected by the release, so the keys are calculated before joining. */
data relcm;
  set relrec;
  if RDOMAIN = 'CM' and IDVAR = 'CMSEQ';
  CMSEQ = input(IDVARVAL, best32.);
  keep STUDYID USUBJID RELID CMSEQ;
run;

data relaeseq;
  set relrec;
  if RDOMAIN = 'AE' and IDVAR = 'AESEQ';
  AESEQ = input(IDVARVAL, best32.);
  keep STUDYID USUBJID RELID AESEQ;
run;

proc sql;
  /* Each adverse event a relationship names, with its reported term. */
  create table relae as
    select r.STUDYID, r.USUBJID, r.RELID, r.AESEQ, a.AETERM
    from relaeseq as r left join ae as a
      on a.STUDYID = r.STUDYID and a.USUBJID = r.USUBJID
         and a.AESEQ = r.AESEQ
    order by r.STUDYID, r.USUBJID, r.RELID, r.AESEQ;
  /* The relationship each medication belongs to. */
  create table cmrel as
    select c.STUDYID, c.USUBJID, c.CMSEQ, c.CMTRT, r.RELID
    from cm as c left join relcm as r
      on r.STUDYID = c.STUDYID and r.USUBJID = c.USUBJID
         and r.CMSEQ = c.CMSEQ;
  create table pairs as
    select m.STUDYID, m.USUBJID, m.CMSEQ, e.AESEQ, e.AETERM
    from cmrel as m inner join relae as e
      on e.STUDYID = m.STUDYID and e.USUBJID = m.USUBJID
         and e.RELID = m.RELID;
quit;

/* The related sequence numbers in numeric order. */
proc sort data=pairs out=byseq;
  by STUDYID USUBJID CMSEQ AESEQ;
run;

data seqs;
  length AESEQS $1000;
  retain AESEQS;
  set byseq;
  by STUDYID USUBJID CMSEQ;
  if first.CMSEQ then AESEQS = '';
  AESEQS = catx(', ', AESEQS, strip(put(AESEQ, best32.)));
  if last.CMSEQ then output;
  keep STUDYID USUBJID CMSEQ AESEQS;
run;

/* The different related terms in alphabetical order, and their number. */
data termpairs;
  set pairs;
  keep STUDYID USUBJID CMSEQ AETERM;
run;

proc sort data=termpairs out=byterm nodupkey;
  by STUDYID USUBJID CMSEQ AETERM;
run;

data terms;
  length AETERMS $1000;
  retain AETERMS AETERMN;
  set byterm;
  by STUDYID USUBJID CMSEQ;
  if first.CMSEQ then do;
    AETERMS = '';
    AETERMN = 0;
  end;
  AETERMS = catx(', ', AETERMS, AETERM);
  AETERMN = AETERMN + 1;
  if last.CMSEQ then output;
  keep STUDYID USUBJID CMSEQ AETERMS AETERMN;
run;

proc sql;
  create table adcm as
    select m.STUDYID, m.USUBJID, m.CMSEQ, m.CMTRT,
           s.AESEQS, t.AETERMS, t.AETERMN
    from cmrel as m
      left join seqs as s
        on s.STUDYID = m.STUDYID and s.USUBJID = m.USUBJID
           and s.CMSEQ = m.CMSEQ
      left join terms as t
        on t.STUDYID = m.STUDYID and t.USUBJID = m.USUBJID
           and t.CMSEQ = m.CMSEQ
    order by m.STUDYID, m.USUBJID, m.CMSEQ;
quit;

proc export data=relae outfile='/app/output/relae.csv' dbms=csv replace; run;
proc export data=adcm outfile='/app/output/adcm.csv' dbms=csv replace; run;
