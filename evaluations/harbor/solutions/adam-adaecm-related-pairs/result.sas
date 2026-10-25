/* opensas reference solution for the yamaa benchmark adam-adaecm-related-pairs.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data ae;
  length STUDYID DOMAIN USUBJID $40 AETERM $200;
  infile "/app/input/ae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char40. DOMAIN : $char40. USUBJID : $char40. AESEQ
        AETERM : $char200.;
run;

data cm;
  length STUDYID DOMAIN USUBJID $40 CMTRT $200;
  infile "/app/input/cm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char40. DOMAIN : $char40. USUBJID : $char40. CMSEQ
        CMTRT : $char200.;
run;

data relrec;
  length STUDYID RDOMAIN USUBJID IDVAR IDVARVAL RELTYPE RELID $40;
  infile "/app/input/relrec.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char40. RDOMAIN : $char40. USUBJID : $char40.
        IDVAR : $char40. IDVARVAL : $char40. RELTYPE : $char40.
        RELID : $char40.;
run;

/* The events and the medications each relationship names, with their
   numeric sequence numbers. Function calls in join predicates were rejected
   by the release, and comparisons between identically named columns of a
   self-join gave incorrect results, so the two member lists are separate
   tables with their keys calculated before joining. */
data relae;
  set relrec;
  if RDOMAIN = 'AE' and IDVAR = 'AESEQ';
  AESEQ = input(IDVARVAL, best32.);
  keep STUDYID USUBJID RELID AESEQ;
run;

data relcm;
  set relrec;
  if RDOMAIN = 'CM' and IDVAR = 'CMSEQ';
  CMSEQ = input(IDVARVAL, best32.);
  keep STUDYID USUBJID RELID CMSEQ;
run;

proc sql;
  /* Every event of a relationship with every medication of the same
     subject's same relationship. */
  create table pairs as
    select e.STUDYID, e.USUBJID, e.AESEQ, m.CMSEQ, e.RELID
    from relae as e inner join relcm as m
      on m.STUDYID = e.STUDYID and m.USUBJID = e.USUBJID
         and m.RELID = e.RELID;
  /* The event's reported term. */
  create table aepairs as
    select p.STUDYID, p.USUBJID, p.AESEQ, a.AETERM, p.CMSEQ, p.RELID
    from pairs as p left join ae as a
      on a.STUDYID = p.STUDYID and a.USUBJID = p.USUBJID
         and a.AESEQ = p.AESEQ;
  /* The medication's reported name. */
  create table adaecm as
    select p.STUDYID, p.USUBJID, p.AESEQ, p.AETERM, p.CMSEQ, c.CMTRT,
           p.RELID
    from aepairs as p left join cm as c
      on c.STUDYID = p.STUDYID and c.USUBJID = p.USUBJID
         and c.CMSEQ = p.CMSEQ
    order by p.STUDYID, p.USUBJID, p.AESEQ, p.CMSEQ, p.RELID;
quit;

proc export data=adaecm outfile='/app/output/adaecm.csv' dbms=csv replace; run;
