proc import datafile='/app/input/morphology.csv' out=morphology dbms=csv replace;
  guessingrows=max;
run;

data mo;
  length DOMAIN $2 STUDYID $200 USUBJID $200 MOSEQ 8 MOTESTCD $8 MOTEST $40 MOORRES $200 MOSTRESC $8 MODTC $10;
  set morphology;
  DOMAIN = 'MO';
  MOSEQ = SEQ;
  MOTESTCD = 'CELLTYPE';
  MOTEST = 'Cell Type';
  MOORRES = RESULT;
  if RESULT = 'normal' then MOSTRESC = 'NORMAL';
  else if RESULT = 'abnormal' then MOSTRESC = 'ABNORMAL';
  MODTC = put(DATE, yymmdd10.);
  keep DOMAIN STUDYID USUBJID MOSEQ MOTESTCD MOTEST MOORRES MOSTRESC MODTC;
run;

proc export data=mo outfile='/app/output/mo.csv' dbms=csv replace;
run;
