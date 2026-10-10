/* opensas reference solution for the yamaa benchmark adam-advs-windows.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data sv;
  length STUDYID USUBJID VISIT SVPRESP SVOCCUR SVSTDTC $1024;
  infile "/app/input/sv.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. VISITNUM VISIT : $char1024. SVPRESP : $char1024. SVOCCUR : $char1024. SVSTDTC : $char1024.;
run;

data vs;
  length STUDYID USUBJID VSTESTCD VISIT VSDTC VSDY $1024;
  infile "/app/input/vs.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. VSTESTCD : $char1024. VSSEQ VISIT : $char1024. VISITNUM VSDTC : $char1024. VSDY : $char1024. VSSTRESN;
run;

data collected;
  length PARAMCD $16 ADT $10 AVISIT $20 ANL01FL $1;
  set vs;
  PARAMCD=VSTESTCD; ADT=VSDTC; ADY=input(VSDY,best32.); AVAL=VSSTRESN;
  if not missing(ADY) then do;
    if ADY<0 then do; AVISIT='SCREENING'; AVISITN=-1; end;
    else if ADY=1 then do; AVISIT='BASELINE'; AVISITN=0; end;
    else if ADY>=2 and ADY<=21 then do; AVISIT='WEEK 2'; AVISITN=2; end;
    else if ADY>=22 and ADY<=42 then do; AVISIT='WEEK 4'; AVISITN=4; end;
    else if ADY>=43 then do; AVISIT='POST-TREATMENT'; AVISITN=99; end;
  end;
run;
proc sort data=collected; by STUDYID USUBJID PARAMCD AVISITN ADY VSSEQ; run;
data flagged; set collected; by STUDYID USUBJID PARAMCD AVISITN; if first.AVISITN and not missing(AVISITN) then ANL01FL='Y'; run;
proc sql;
  create table maxima as select STUDYID,USUBJID,max(VSSEQ) as maximum from collected group by STUDYID,USUBJID;
  create table absent as select a.*,b.maximum from sv as a left join maxima as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID
  where a.VISIT in ('SCREENING','BASELINE','WEEK 2','WEEK 4') and not exists(select * from collected as c where a.STUDYID=c.STUDYID and a.USUBJID=c.USUBJID and c.PARAMCD='SYSBP' and a.VISIT=c.AVISIT);
quit;
proc sort data=absent; by STUDYID USUBJID VISITNUM; run;
data expected;
  length PARAMCD $16 ADT $10 AVISIT $20 ANL01FL $1;
  set absent; by STUDYID USUBJID;
  if first.USUBJID then counter=coalesce(maximum,0); counter+1; VSSEQ=counter;
  PARAMCD='SYSBP'; AVISIT=VISIT;
  select(VISIT); when('SCREENING') AVISITN=-1; when('BASELINE') AVISITN=0; when('WEEK 2') AVISITN=2; when('WEEK 4') AVISITN=4; otherwise; end;
  ADY=.; AVAL=.;
run;
data result; set flagged expected; run;

proc sql;
  create table final as select STUDYID, USUBJID, PARAMCD, VSSEQ, VISIT, VISITNUM, ADT, ADY, AVAL, AVISIT, AVISITN, ANL01FL from result;
quit;
proc export data=final outfile="/app/output/advs.csv" dbms=csv replace; run;
