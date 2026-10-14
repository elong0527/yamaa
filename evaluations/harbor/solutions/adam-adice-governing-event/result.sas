/* opensas reference: select priority before date and source sequence. */
data events;
  length STUDYID USUBJID $80 ADECOD1 $8 date_text $10;
  infile "/app/input/ice_raw.csv" dsd dlm="," firstobs=2 truncover;
  input STUDYID : $80. USUBJID : $80. ASEQ ADECOD1 : $8.
        date_text : $10.;
  ASTDT=input(date_text,yymmdd10.);
  if ADECOD1 in ('ICE04','ICE05') then ICEPRIO=1;
  else if ADECOD1='ICE01' then ICEPRIO=2;
  else if ADECOD1='ICE02' then ICEPRIO=3;
  else if ADECOD1='ICE03' then ICEPRIO=4;
run;
proc sort data=events;
  by STUDYID USUBJID ICEPRIO ASTDT ASEQ;
run;
data ranked;
  length GOVFL SUPERFL $1 GOVTYPE $8;
  set events;
  by STUDYID USUBJID;
  retain ICERANK GOVTYPE GOVDT GOVSEQ;
  if first.USUBJID then do;
    ICERANK=0; GOVTYPE=ADECOD1; GOVDT=ASTDT; GOVSEQ=ASEQ;
  end;
  ICERANK+1;
  if ICERANK=1 then GOVFL='Y';
  else SUPERFL='Y';
  format ASTDT GOVDT yymmdd10.;
run;
proc sql;
  create table result as select STUDYID, USUBJID, ASEQ, ADECOD1, ASTDT,
    ICEPRIO, ICERANK, GOVFL, SUPERFL, GOVTYPE, GOVDT, GOVSEQ
  from ranked order by STUDYID, USUBJID, ASEQ;
quit;
proc export data=result outfile="/app/output/adice.csv" dbms=csv replace;
run;
