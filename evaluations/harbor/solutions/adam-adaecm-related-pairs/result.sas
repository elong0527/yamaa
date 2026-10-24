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

proc sql;
  /* Every event of a relationship with every medication of the same
     subject's same relationship, with their terms. */
  create table adaecm as
    select e.STUDYID, e.USUBJID,
           input(e.IDVARVAL, best32.) as AESEQ, a.AETERM,
           input(m.IDVARVAL, best32.) as CMSEQ, c.CMTRT, e.RELID
    from relrec as e
      inner join relrec as m
        on m.STUDYID = e.STUDYID and m.USUBJID = e.USUBJID
           and m.RELID = e.RELID
      left join ae as a
        on a.STUDYID = e.STUDYID and a.USUBJID = e.USUBJID
           and a.AESEQ = input(e.IDVARVAL, best32.)
      left join cm as c
        on c.STUDYID = m.STUDYID and c.USUBJID = m.USUBJID
           and c.CMSEQ = input(m.IDVARVAL, best32.)
    where e.RDOMAIN = 'AE' and e.IDVAR = 'AESEQ'
      and m.RDOMAIN = 'CM' and m.IDVAR = 'CMSEQ'
    order by 1, 2, 3, 5;
quit;

proc export data=adaecm outfile='/app/output/adaecm.csv' dbms=csv replace; run;
