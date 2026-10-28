proc import datafile='/app/input/lab_transfer.csv' out=lab_transfer dbms=csv replace;
  guessingrows=max;
run;

proc sort data=lab_transfer out=lab_sorted;
  by Protocol Subject Collected Sample;
run;

data pc;
  length DOMAIN $2 STUDYID $200 USUBJID $200 PCSEQ 8 PCREFID $200
         PCTESTCD $8 PCTEST $40 PCORRES $200 PCORRESU $40
         PCSTRESC $200 PCSTRESN 8 PCSTRESU $40 PCSPEC $40
         PCDTC $40 PCTPT $40 PCTPTNUM 8;
  set lab_sorted;
  by Protocol Subject;
  retain PCSEQ;
  if first.Subject then PCSEQ=0;
  PCSEQ=PCSEQ+1;
  DOMAIN='PC';
  STUDYID=Protocol;
  USUBJID=Subject;
  PCREFID=Sample;
  PCTESTCD='ZX201';
  PCTEST='ZX-201';
  PCORRES=Result;
  PCORRESU=Unit;
  PCSTRESC=PCORRES;
  PCSTRESN=.;
  if Result ne 'BLQ' then PCSTRESN=input(Result,best32.);
  PCSTRESU=PCORRESU;
  PCSPEC='PLASMA';
  PCDTC=Collected;
  PCTPT=Nominal;
  PCTPTNUM=.;
  if Nominal='PRE' then PCTPTNUM=0;
  else if Nominal ne 'UNSCHED' then PCTPTNUM=input(compress(Nominal,'H'),best32.);
  keep DOMAIN STUDYID USUBJID PCSEQ PCREFID PCTESTCD PCTEST PCORRES PCORRESU
       PCSTRESC PCSTRESN PCSTRESU PCSPEC PCDTC PCTPT PCTPTNUM;
run;

proc export data=pc outfile='/app/output/pc.csv' dbms=csv replace;
run;
