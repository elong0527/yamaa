/* opensas reference solution for the yamaa benchmark adam-adcm-atc-classes.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data atc_dict;
  length ATCLEVEL ATCCODE ATCNAME $1024;
  infile "/app/input/atc_dict.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input ATCLEVEL : $char1024. ATCCODE : $char1024. ATCNAME : $char1024.;
run;

data cm;
  length STUDYID USUBJID CMTRT CMDECOD $1024;
  infile "/app/input/cm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. CMSEQ CMTRT : $char1024. CMDECOD : $char1024.;
run;

data facm;
  length STUDYID USUBJID FALNKGRP FATESTCD FAORRES $1024;
  infile "/app/input/facm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. FASEQ FALNKGRP : $char1024. CMSEQ FATESTCD : $char1024. FAORRES : $char1024.;
run;

proc sql;
  create table result as
  select a.*, f1.FAORRES as ATC1CD, d1.ATCNAME as ATC1, f2.FAORRES as ATC2CD, d2.ATCNAME as ATC2, f3.FAORRES as ATC3CD, d3.ATCNAME as ATC3, f4.FAORRES as ATC4CD, d4.ATCNAME as ATC4 from cm as a
left join facm as f1 on a.STUDYID=f1.STUDYID and a.USUBJID=f1.USUBJID and a.CMSEQ=f1.CMSEQ and f1.FATESTCD='ATC1' and a.CMDECOD ne ''
left join atc_dict as d1 on d1.ATCLEVEL='1' and d1.ATCCODE=f1.FAORRES
left join facm as f2 on a.STUDYID=f2.STUDYID and a.USUBJID=f2.USUBJID and a.CMSEQ=f2.CMSEQ and f2.FATESTCD='ATC2' and a.CMDECOD ne ''
left join atc_dict as d2 on d2.ATCLEVEL='2' and d2.ATCCODE=f2.FAORRES
left join facm as f3 on a.STUDYID=f3.STUDYID and a.USUBJID=f3.USUBJID and a.CMSEQ=f3.CMSEQ and f3.FATESTCD='ATC3' and a.CMDECOD ne ''
left join atc_dict as d3 on d3.ATCLEVEL='3' and d3.ATCCODE=f3.FAORRES
left join facm as f4 on a.STUDYID=f4.STUDYID and a.USUBJID=f4.USUBJID and a.CMSEQ=f4.CMSEQ and f4.FATESTCD='ATC4' and a.CMDECOD ne ''
left join atc_dict as d4 on d4.ATCLEVEL='4' and d4.ATCCODE=f4.FAORRES;
quit;

proc sql;
  create table final as select STUDYID, USUBJID, CMSEQ, CMTRT, CMDECOD, ATC1, ATC2, ATC3, ATC4, ATC1CD, ATC2CD, ATC3CD, ATC4CD from result;
quit;
proc export data=final outfile="/app/output/adcm.csv" dbms=csv replace; run;
