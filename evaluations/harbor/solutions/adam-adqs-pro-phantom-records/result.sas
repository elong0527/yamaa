/* opensas reference solution for the yamaa benchmark adam-adqs-pro-phantom-records.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adsl;
  length STUDYID USUBJID SAFFL DCTREAS $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. RANDDT : yymmdd10. SAFFL : $char1024. EOTDT : yymmdd10. DCTREAS : $char1024. DTHDT : yymmdd10.;
  /* Days counted from randomization as day 1, as the planned days are. */
  if not missing(EOTDT) then EOTDY = EOTDT - RANDDT + 1;
  if not missing(DTHDT) then DTHDY = DTHDT - RANDDT + 1;
run;

data schedule;
  length STUDYID VISIT PARCAT2 PARAMCD PARAM $1024;
  infile "/app/input/schedule.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. VISITNUM VISIT : $char1024. VISITDY PARCAT2 : $char1024. PARAMCD : $char1024. PARAM : $char1024.;
run;

data qs;
  length STUDYID USUBJID QSCAT QSTESTCD QSTEST QSSTAT QSREASND VISIT $1024;
  infile "/app/input/qs.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. QSSEQ QSCAT : $char1024. QSTESTCD : $char1024. QSTEST : $char1024. QSSTRESN QSSTAT : $char1024. QSREASND : $char1024. VISITNUM VISIT : $char1024.;
run;

/* Every randomized subject at every planned timepoint and parameter, with
   the collected item record when there is one. */
proc sql;
  create table frame as select a.STUDYID,a.USUBJID,a.SAFFL,a.DCTREAS,a.EOTDY,a.DTHDY,b.VISITNUM,b.VISIT,b.VISITDY,b.PARCAT2,b.PARAMCD,b.PARAM from adsl as a inner join schedule as b on a.STUDYID=b.STUDYID;
  create table joined as select f.*,i.QSSEQ,i.QSSTRESN,i.QSSTAT,i.QSREASND from frame as f left join qs as i on f.STUDYID=i.STUDYID and f.USUBJID=i.USUBJID and f.VISITNUM=i.VISITNUM and f.PARAMCD=i.QSTESTCD;
quit;

/* Per subject and visit: the sum and count of the answered items, how many
   items were collected, and the reason the items carry. */
proc sort data=qs out=items; by USUBJID VISITNUM; run;
data visitagg;
  length ITEMREAS $1024;
  set items;
  by USUBJID VISITNUM;
  retain ITEMSUM ITEMANS ITEMCOL ITEMREAS;
  if first.VISITNUM then do; ITEMSUM=.; ITEMANS=0; ITEMCOL=0; ITEMREAS=''; end;
  ITEMCOL=ITEMCOL+1;
  if not missing(QSSTRESN) then do; ITEMSUM=sum(ITEMSUM,QSSTRESN); ITEMANS=ITEMANS+1; end;
  if not missing(QSREASND) and QSREASND > ITEMREAS then ITEMREAS=QSREASND;
  if last.VISITNUM then output;
  keep USUBJID VISITNUM ITEMSUM ITEMANS ITEMCOL ITEMREAS;
run;

proc sort data=joined; by USUBJID VISITNUM PARAMCD; run;
data final;
  length DTYPE AREASND PROEX1FL PROEX2FL PROSCMFL $1024;
  merge joined(in=f) visitagg;
  by USUBJID VISITNUM;
  if f;
  AVAL=.;
  DTYPE='';
  if PARCAT2='ITEM' then do;
    AVAL=QSSTRESN;
    if missing(QSSEQ) then DTYPE='PHANTOM';
  end;
  else do;
    if ITEMANS=2 then AVAL=ITEMSUM;
    if missing(ITEMCOL) or ITEMCOL=0 then DTYPE='PHANTOM';
  end;
  AREASND='';
  if missing(AVAL) then do;
    if DTYPE='PHANTOM' then do;
      if SAFFL='N' then AREASND='NOT TREATED';
      else if not missing(EOTDY) and VISITDY > EOTDY then AREASND=DCTREAS;
    end;
    else if PARCAT2='ITEM' then AREASND=QSREASND;
    else if not missing(ITEMREAS) then AREASND=ITEMREAS;
    else AREASND='NOT CALCULABLE';
  end;
  /* Clinical benefit expects the measure until death; safety and
     tolerability from a treated subject until treatment ends or death. */
  PROEX1FL='Y';
  if not missing(DTHDY) and VISITDY > DTHDY then PROEX1FL='';
  PROEX2FL='';
  if SAFFL='Y' and (missing(EOTDY) or VISITDY <= EOTDY) and (missing(DTHDY) or VISITDY <= DTHDY) then PROEX2FL='Y';
  PROSCMFL='';
  if not missing(AVAL) then PROSCMFL='Y';
run;

proc sql;
  create table out as select STUDYID, USUBJID, PARCAT2, PARAMCD, PARAM, VISIT as AVISIT, VISITNUM as AVISITN, AVAL, DTYPE, QSSTAT, QSREASND, AREASND, PROEX1FL, PROEX2FL, PROSCMFL from final order by USUBJID, AVISITN, PARAMCD;
quit;
proc export data=out outfile="/app/output/adqs.csv" dbms=csv replace; run;
