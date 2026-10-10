/* SAS language reference solution for the yamaa benchmark adam-adrs-response-prep.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data adrs_raw;
  length STUDYID USUBJID ADT RANDDY AVALC $1024;
  infile "/app/input/adrs_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. ASEQ ADT : $char1024. RANDDY : $char1024. AVALC : $char1024.;
run;

data adsl;
  length STUDYID USUBJID NTXSTDT $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. NTXSTDT : $char1024.;
run;

proc sql; create table joined as select a.*,b.NTXSTDT from adrs_raw as a left join adsl as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID; quit;
data categories;
  length BORCAT $16;
  set joined;
  if AVALC in ('CR','PR','SD','NON-CR/NON-PD','PD','NE') and (missing(NTXSTDT) or ADT<NTXSTDT) then do;
    BORCAT=AVALC;
    if AVALC in ('SD','NON-CR/NON-PD') and (missing(RANDDY) or input(RANDDY,best32.)<42) then BORCAT='NE';
  end;
  select(BORCAT); when('CR') BORPRI=1; when('PR') BORPRI=2; when('SD') BORPRI=3; when('NON-CR/NON-PD') BORPRI=4; when('PD') BORPRI=5; when('NE') BORPRI=6; otherwise BORPRI=.; end;
  unusable=missing(BORPRI);
run;
proc sort data=categories; by STUDYID USUBJID unusable BORPRI ADT ASEQ; run;
data result;
  set categories; by STUDYID USUBJID;
  if first.USUBJID then sequence=0;
  if not missing(BORPRI) then do; sequence+1; BORSEQ=sequence; end;
run;

proc sql;
  create table final as select STUDYID, USUBJID, ASEQ, ADT, RANDDY, AVALC, BORCAT, BORPRI, BORSEQ from result;
quit;
proc export data=final outfile="/app/output/adrs.csv" dbms=csv replace; run;
