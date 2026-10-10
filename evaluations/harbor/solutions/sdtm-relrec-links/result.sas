/* opensas reference solution for the yamaa benchmark sdtm-relrec-links.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data ae;
  length STUDYID USUBJID AETERM AELNKID1 AELNKID2 $1024;
  infile "/app/input/ae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AETERM : $char1024. AELNKID1 : $char1024. AELNKID2 : $char1024.;
run;

data cm;
  length STUDYID USUBJID CMTRT CMLNKID1 CMLNKID2 $1024;
  infile "/app/input/cm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. CMSEQ CMTRT : $char1024. CMLNKID1 : $char1024. CMLNKID2 : $char1024.;
run;

data aelinks; set ae; length RDOMAIN IDVAR IDVARVAL RELTYPE RELID $1024; RDOMAIN='AE'; IDVAR='AESEQ'; IDVARVAL=strip(put(AESEQ,best32.)); RELTYPE='';
if not missing(AELNKID1) then do; RELID=AELNKID1; output; end;
if not missing(AELNKID2) then do; RELID=AELNKID2; output; end;
run;
data cmlinks; set cm; length RDOMAIN IDVAR IDVARVAL RELTYPE RELID $1024; RDOMAIN='CM'; IDVAR='CMSEQ'; IDVARVAL=strip(put(CMSEQ,best32.)); RELTYPE='';
if not missing(CMLNKID1) then do; RELID=CMLNKID1; output; end;
if not missing(CMLNKID2) then do; RELID=CMLNKID2; output; end;
run;
data result; set aelinks cmlinks; run;
proc sql;
  create table final as select STUDYID, USUBJID, RDOMAIN, IDVAR, IDVARVAL, RELTYPE, RELID from result;
quit;
proc export data=final outfile="/app/output/relrec.csv" dbms=csv replace; run;
