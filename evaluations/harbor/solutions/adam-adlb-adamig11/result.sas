proc import datafile='/app/input/adsl.csv' out=adsl dbms=csv replace;
  getnames=yes;
run;

proc import datafile='/app/input/lb.csv' out=lb dbms=csv replace;
  getnames=yes;
run;

proc sql;
  create table source as
  select l.*, a.TRTSDT, a.TRT01A
  from lb as l
  left join adsl as a
    on l.STUDYID=a.STUDYID and l.USUBJID=a.USUBJID;
quit;

data derived;
  length PARAMCD $8 PARAM $40 AVALU $20 ABLFL $1;
  set source;
  PARAMCD=LBTESTCD;
  PARAM='Alanine Aminotransferase';
  ADT=LBDTC;
  _TRTSDT=TRTSDT;
  if ADT ne . and _TRTSDT ne . then do;
    if ADT < _TRTSDT then ADY=ADT-_TRTSDT;
    else ADY=ADT-_TRTSDT+1;
  end;
  AVAL=LBSTRESN;
  AVALU=LBSTRESU;
  ABLFL=LBBLFL;
  format ADT yymmdd10.;
run;

proc sql;
  create table adlb_joined as
  select d.STUDYID, d.USUBJID, d.PARAMCD, d.PARAM,
         d.ADT format=yymmdd10., d.TRT01A, d.ADY, d.AVAL,
         d.AVALU, d.ABLFL, b.AVAL as BASE,
         case when d.AVAL is not null and b.AVAL is not null then d.AVAL-b.AVAL else . end as CHG
  from derived as d
  left join derived as b
    on d.STUDYID=b.STUDYID and d.USUBJID=b.USUBJID
       and d.PARAMCD=b.PARAMCD and b.ABLFL='Y'
  order by d.STUDYID, d.USUBJID, d.PARAMCD, d.ADT;
quit;

data adlb;
  set adlb_joined;
  format ADT yymmdd10.;
run;

proc sql;
  create table adlb_csv as
  select STUDYID, USUBJID, PARAMCD, PARAM,
         put(ADT,yymmdd10.) as ADT, TRT01A, ADY, AVAL, AVALU,
         ABLFL, BASE, CHG
  from adlb;
quit;

proc export data=adlb_csv outfile='/app/output/adlb.csv' dbms=csv replace;
run;
