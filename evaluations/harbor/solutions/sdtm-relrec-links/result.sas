/* opensas reference solution for the yamaa benchmark sdtm-relrec-links.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data ae;
  length STUDYID USUBJID AESPID AETERM $1024;
  infile "/app/input/ae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AESPID : $char1024. AETERM : $char1024.;
run;

data cm;
  length STUDYID USUBJID CMTRT CMAENO1 CMAENO2 $1024;
  infile "/app/input/cm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. CMSEQ CMTRT : $char1024. CMAENO1 : $char1024. CMAENO2 : $char1024.;
run;

data refs; set cm; length AENO $1024;
if not missing(CMAENO1) then do; AENO=strip(CMAENO1); output; end;
if not missing(CMAENO2) then do; AENO=strip(CMAENO2); output; end;
keep STUDYID USUBJID CMSEQ AENO;
run;

proc sql;
  create table matched as
  select r.STUDYID, r.USUBJID, a.AESEQ, r.CMSEQ, a.AESPID as MATCHED_AENO
  from refs as r left join ae as a
    on r.STUDYID=a.STUDYID and r.USUBJID=a.USUBJID and r.AENO=strip(a.AESPID)
  order by r.STUDYID, r.USUBJID, a.AESEQ, r.CMSEQ;
quit;

/* A populated reference must name an event before any output is written. */
data pairs; set matched;
if missing(MATCHED_AENO) then abort return 1;
drop MATCHED_AENO;
run;

data numbered; set pairs; by STUDYID USUBJID;
if first.USUBJID then RELNUM=0;
RELNUM+1;
run;

data result; set numbered;
length RDOMAIN IDVAR IDVARVAL RELTYPE RELID $1024;
RELTYPE=''; RELID=strip(put(RELNUM,best32.));
RDOMAIN='AE'; IDVAR='AESEQ'; IDVARVAL=strip(put(AESEQ,best32.)); output;
RDOMAIN='CM'; IDVAR='CMSEQ'; IDVARVAL=strip(put(CMSEQ,best32.)); output;
run;

proc sql;
  create table final as select STUDYID, USUBJID, RDOMAIN, IDVAR, IDVARVAL, RELTYPE, RELID
  from result order by STUDYID, USUBJID, RELNUM, RDOMAIN;
quit;
proc export data=final outfile="/app/output/relrec.csv" dbms=csv replace; run;
