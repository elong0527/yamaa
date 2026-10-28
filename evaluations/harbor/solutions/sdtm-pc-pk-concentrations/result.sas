proc import datafile='/app/input/lab_transfer.csv' out=lab_transfer dbms=csv replace;
  guessingrows=max;
  getnames=yes;
run;

data pc_base;
  length DOMAIN $2 STUDYID $40 USUBJID $80 PCREFID $40 PCTESTCD $8 PCTEST $40
         PCORRES $80 PCORRESU $8 PCSTRESC $80 PCSTRESU $8 PCSPEC $20
         PCDTC $30 PCTPT $30;
  set lab_transfer;
  DOMAIN='PC';
  STUDYID=Protocol;
  USUBJID=Subject;
  PCREFID=Sample;
  PCTESTCD='ZX201';
  PCTEST='ZX-201';
  PCORRES=Result;
  PCORRESU='ng/mL';
  PCSTRESC=Result;
  PCSTRESN=.;
  if Result ne 'BLQ' then PCSTRESN=input(Result,best32.);
  PCSTRESU='ng/mL';
  PCSPEC='PLASMA';
  PCDTC=Collected;
  PCTPT=Nominal;
  PCTPTNUM=.;
  if Nominal='PRE' then PCTPTNUM=0;
  else if Nominal ne 'UNSCHED' then PCTPTNUM=input(compress(Nominal,'H'),best32.);
run;

proc sort data=pc_base;
  by STUDYID USUBJID PCDTC PCREFID;
run;

data pc;
  retain DOMAIN STUDYID USUBJID PCSEQ PCREFID PCTESTCD PCTEST PCORRES PCORRESU
         PCSTRESC PCSTRESN PCSTRESU PCSPEC PCDTC PCTPT PCTPTNUM;
  set pc_base;
  by STUDYID USUBJID;
  if first.USUBJID then PCSEQ=0;
  PCSEQ+1;
  keep DOMAIN STUDYID USUBJID PCSEQ PCREFID PCTESTCD PCTEST PCORRES PCORRESU
       PCSTRESC PCSTRESN PCSTRESU PCSPEC PCDTC PCTPT PCTPTNUM;
run;

proc export data=pc outfile='/app/output/pc.csv' dbms=csv replace;
run;
