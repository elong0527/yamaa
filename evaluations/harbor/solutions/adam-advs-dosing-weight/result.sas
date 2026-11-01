proc import datafile='/app/input/adsl.csv' out=adsl dbms=csv replace;
  getnames=yes;
  guessingrows=max;
run;

proc import datafile='/app/input/vs.csv' out=vs dbms=csv replace;
  getnames=yes;
  guessingrows=max;
run;

proc sql;
  create table cycle_weights as
  select v.STUDYID, v.USUBJID, v.VSSEQ as ASEQ,
         v.VISITNUM as AVISITN, v.VSDTC, v.VSSTRESN as AVAL,
         a.HEIGHTBL
  from vs as v left join adsl as a
    on v.STUDYID=a.STUDYID and v.USUBJID=a.USUBJID;
quit;

proc sort data=cycle_weights;
  by STUDYID USUBJID AVISITN ASEQ;
run;

data derived;
  set cycle_weights;
  by STUDYID USUBJID;
  retain ANCHORWT;
  if first.USUBJID then ANCHORWT=.;
  if not missing(AVAL) then do;
    if missing(ANCHORWT) then ANCHORWT=AVAL;
    else if 10*abs(AVAL-ANCHORWT) >= ANCHORWT-1e-10 then ANCHORWT=AVAL;
  end;
  ADT=put(VSDTC,yymmdd10.);
  BSA=round(sqrt(ANCHORWT*HEIGHTBL/3600),0.01);
run;

proc sql;
  create table advs as
  select STUDYID, USUBJID, ASEQ, AVISITN, ADT, AVAL, ANCHORWT, BSA
  from derived
  order by STUDYID, USUBJID, AVISITN, ASEQ;
quit;

proc export data=advs outfile='/app/output/advs.csv' dbms=csv replace;
run;
