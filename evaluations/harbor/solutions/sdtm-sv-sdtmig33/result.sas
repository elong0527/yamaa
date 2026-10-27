/* SDTMIG 3.3 Subject Visits: preserve actual date ranges and visit numbers. */
proc import datafile="/app/input/visits.csv" out=visits dbms=csv replace;
  getnames=yes;
  guessingrows=max;
run;

data sv;
  length DOMAIN $2 STUDYID $200 USUBJID $200 VISITNUM 8
         VISIT $200 SVSTDTC $10 SVENDTC $10 SVUPDES $200;
  set visits(rename=(VISITNO=VISITNUM NAME=VISIT REASON=SVUPDES));
  DOMAIN="SV";
  /* IMPORT reads ISO dates as SAS dates; SV dates use ISO 8601 text. */
  SVSTDTC=put(START,yymmdd10.);
  SVENDTC=put(END,yymmdd10.);
  /* Integer visit numbers identify planned visits in this source. */
  if VISITNUM=int(VISITNUM) then SVUPDES="";
  keep DOMAIN STUDYID USUBJID VISITNUM VISIT SVSTDTC SVENDTC SVUPDES;
run;

proc sort data=sv;
  by STUDYID USUBJID VISITNUM;
run;

proc export data=sv outfile="/app/output/sv.csv" dbms=csv replace;
run;
