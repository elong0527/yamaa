/* SAS language reference solution for the yamaa benchmark sdtm-relrec-dataset-level.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data ae;
  length STUDYID USUBJID AETERM AELNKID $1024;
  infile "/app/input/ae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AETERM : $char1024. AELNKID : $char1024.;
run;

data cm;
  length STUDYID USUBJID CMTRT CMLNKID $1024;
  infile "/app/input/cm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. CMSEQ CMTRT : $char1024. CMLNKID : $char1024.;
run;

data tr;
  length STUDYID USUBJID TRTESTCD TRLNKID $1024;
  infile "/app/input/tr.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. TRSEQ TRTESTCD : $char1024. TRLNKID : $char1024.;
run;

data tu;
  length STUDYID USUBJID TUTESTCD TULNKID $1024;
  infile "/app/input/tu.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. TUSEQ TUTESTCD : $char1024. TULNKID : $char1024.;
run;

data aelinks; set ae; length RDOMAIN IDVAR IDVARVAL RELTYPE RELID $1024; RDOMAIN='AE'; IDVAR='AESEQ'; IDVARVAL=strip(put(AESEQ,best32.)); RELTYPE='';
if not missing(AELNKID) then do; RELID=AELNKID; output; end;
run;
data cmlinks; set cm; length RDOMAIN IDVAR IDVARVAL RELTYPE RELID $1024; RDOMAIN='CM'; IDVAR='CMSEQ'; IDVARVAL=strip(put(CMSEQ,best32.)); RELTYPE='';
if not missing(CMLNKID) then do; RELID=CMLNKID; output; end;
run;
proc sql; create table studies as select distinct STUDYID from tu union select distinct STUDYID from tr; quit;
data datasets; set studies; length USUBJID RDOMAIN IDVAR IDVARVAL RELTYPE RELID $1024; USUBJID=''; IDVARVAL=''; RELID='1'; RDOMAIN='TU'; IDVAR='TULNKID'; RELTYPE='ONE'; output; RDOMAIN='TR'; IDVAR='TRLNKID'; RELTYPE='MANY'; output; run;
data result; set datasets aelinks cmlinks; run;
proc sql;
  create table final as select STUDYID, USUBJID, RDOMAIN, IDVAR, IDVARVAL, RELTYPE, RELID from result;
quit;
proc export data=final outfile="/app/output/relrec.csv" dbms=csv replace; run;
