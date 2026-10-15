/* opensas reference: first qualifying onset defines each term's risk. */
data subjects;
  length STUDYID USUBJID $80 TRT01A $20 start_text end_text $10;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover;
  input STUDYID : $80. USUBJID : $80. TRT01A : $20.
        start_text : $10. end_text : $10.;
  TRTSDT=input(start_text,yymmdd10.); TRTEDT=input(end_text,yymmdd10.);
run;
data events;
  length STUDYID USUBJID $80 AEDECOD $20 date_text $10;
  infile "/app/input/adae.csv" dsd dlm="," firstobs=2 truncover;
  input STUDYID : $80. USUBJID : $80. ASEQ AEDECOD : $20.
        date_text : $10.;
  ASTDT=input(date_text,yymmdd10.);
run;
data population;
  length AEDECOD $20;
  set subjects;
  AEDECOD='DIARRHOEA'; output;
  AEDECOD='HEADACHE'; output;
run;
proc sql;
  create table selected as select p.STUDYID, p.USUBJID, p.TRT01A,
    p.AEDECOD, p.TRTSDT, p.TRTEDT, min(e.ASTDT) as FIRSTDT
  from population p left join events e
    on p.STUDYID=e.STUDYID and p.USUBJID=e.USUBJID
    and p.AEDECOD=e.AEDECOD and e.ASTDT >= p.TRTSDT
    and e.ASTDT <= p.TRTEDT+30
  group by p.STUDYID, p.USUBJID, p.TRT01A, p.AEDECOD, p.TRTSDT, p.TRTEDT;
quit;
data risk;
  set selected;
  EVENTN=not missing(FIRSTDT);
  if EVENTN then RISKDAYS=FIRSTDT-TRTSDT+1;
  else RISKDAYS=TRTEDT+30-TRTSDT+1;
  format FIRSTDT yymmdd10.;
run;
proc sql;
  create table risk_output as select STUDYID, USUBJID, TRT01A, AEDECOD,
    put(FIRSTDT,yymmdd10.) as FIRSTDT, EVENTN, RISKDAYS from risk
  order by STUDYID, USUBJID, AEDECOD;
  create table totals as select STUDYID, TRT01A, AEDECOD,
    sum(EVENTN) as NSUBJ, sum(RISKDAYS) as PERSONDAYS
  from risk group by STUDYID, TRT01A, AEDECOD;
quit;
data rates;
  set totals;
  PERSONYRS=PERSONDAYS/365.25; EAIR100=100*NSUBJ/PERSONYRS;
run;
proc sql;
  create table rate_output as select STUDYID, TRT01A, AEDECOD, NSUBJ,
    PERSONDAYS, PERSONYRS, EAIR100 from rates
  order by STUDYID, TRT01A, AEDECOD;
quit;
proc export data=risk_output outfile="/app/output/adrisk.csv"
  dbms=csv replace; run;
proc export data=rate_output outfile="/app/output/adinc.csv"
  dbms=csv replace; run;
