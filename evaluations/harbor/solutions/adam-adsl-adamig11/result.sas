proc import datafile="/app/input/adsl_raw.csv" out=adsl_raw dbms=csv replace;
  getnames=yes;
run;

data adsl;
  length STUDYID $20 USUBJID $40 TRT01A $40 AGE 8 HEIGHTBL 8 TRTSDT $10 TRTSDTM $19 TRTSTMF $1;
  set adsl_raw;
  if not missing(TRTSDTC) then do;
    TRTSDTM=strip(TRTSDTC);
    /* Seconds are collected for other subjects: flag imputed ones. */
    if length(TRTSDTM)=16 then do; TRTSDTM=cats(TRTSDTM,':00'); TRTSTMF='S'; end;
    TRTSDT=substr(TRTSDTM,1,10);
  end;
  keep STUDYID USUBJID TRT01A AGE HEIGHTBL TRTSDT TRTSDTM TRTSTMF;
run;

proc export data=adsl outfile="/app/output/adsl.csv" dbms=csv replace;
run;
