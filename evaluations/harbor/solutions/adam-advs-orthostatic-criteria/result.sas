/* opensas reference solution for the yamaa benchmark adam-advs-orthostatic-criteria.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data vs;
  length STUDYID USUBJID VISIT VSTESTCD VSPOS VSTPT $1024;
  infile "/app/input/vs.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. VSSEQ VISIT : $char1024. VSTESTCD : $char1024. VSPOS : $char1024. VSTPT : $char1024. VSSTRESN;
run;

/* A test with more than one supine reading, or more than one reading
   after three minutes standing, at one visit stops the run rather
   than choosing one: count the readings of each kind first, before
   the aggregations below can pick one. */
proc sql;
  create table dup_readings as
    select STUDYID, USUBJID, VISIT, VSTESTCD, count(*) as N
      from (select STUDYID, USUBJID, VISIT, VSTESTCD, VSPOS,
              case when VSPOS='SUPINE' then '' else VSTPT end as TPTKEY
              from vs
              where VSTESTCD in ('SYSBP','DIABP','HR')
                and (VSPOS='SUPINE' or (VSPOS='STANDING' and VSTPT='AFTER 3 MIN STANDING'))) as r
      group by STUDYID, USUBJID, VISIT, VSTESTCD, VSPOS, TPTKEY
      having count(*) > 1;
quit;

data _null_;
  set dup_readings;
  abort;
run;

/* One row per subject and visit: the supine reading and the reading
   after three minutes standing of each test. The reading after one
   minute standing is never used. */
proc sql;
  create table visits as select STUDYID, USUBJID, VISIT as AVISIT,
    max(case when VSTESTCD='SYSBP' and VSPOS='SUPINE' then VSSTRESN end) as SYSBP_SUP,
    max(case when VSTESTCD='SYSBP' and VSPOS='STANDING' and VSTPT='AFTER 3 MIN STANDING' then VSSTRESN end) as SYSBP_STD,
    max(case when VSTESTCD='DIABP' and VSPOS='SUPINE' then VSSTRESN end) as DIABP_SUP,
    max(case when VSTESTCD='DIABP' and VSPOS='STANDING' and VSTPT='AFTER 3 MIN STANDING' then VSSTRESN end) as DIABP_STD,
    max(case when VSTESTCD='HR' and VSPOS='SUPINE' then VSSTRESN end) as HR_SUP,
    max(case when VSTESTCD='HR' and VSPOS='STANDING' and VSTPT='AFTER 3 MIN STANDING' then VSSTRESN end) as HR_STD
    from vs group by STUDYID, USUBJID, VISIT;
quit;

/* Changes are standing minus supine. Either blood-pressure fall is
   enough for Y, even when the other change is missing; N needs both
   changes. A missing value never meets a threshold in these tests
   because each comparison first checks the value is present. */
data flagged;
  length OHYPO OTACHY $1;
  set visits;
  SYSBP_CHG=SYSBP_STD-SYSBP_SUP;
  DIABP_CHG=DIABP_STD-DIABP_SUP;
  HR_CHG=HR_STD-HR_SUP;
  if (not missing(SYSBP_CHG) and SYSBP_CHG<=-20) or (not missing(DIABP_CHG) and DIABP_CHG<=-10) then OHYPO='Y';
  else if not missing(SYSBP_CHG) and not missing(DIABP_CHG) then OHYPO='N';
  if not missing(HR_CHG) and HR_CHG>=30 then OTACHY='Y';
  else if not missing(HR_CHG) then OTACHY='N';
run;

proc sql;
  create table result as
    select STUDYID, USUBJID, AVISIT, 'DIABPOC' as PARAMCD, 'Diastolic Blood Pressure Change on Standing (mmHg)' as PARAM, DIABP_CHG as AVAL, '' as AVALC from flagged where not missing(DIABP_CHG)
    union all
    select STUDYID, USUBJID, AVISIT, 'SYSBPOC' as PARAMCD, 'Systolic Blood Pressure Change on Standing (mmHg)' as PARAM, SYSBP_CHG as AVAL, '' as AVALC from flagged where not missing(SYSBP_CHG)
    union all
    select STUDYID, USUBJID, AVISIT, 'HROC' as PARAMCD, 'Heart Rate Change on Standing (beats/min)' as PARAM, HR_CHG as AVAL, '' as AVALC from flagged where not missing(HR_CHG)
    union all
    select STUDYID, USUBJID, AVISIT, 'OHYPO' as PARAMCD, 'Orthostatic Hypotension' as PARAM, . as AVAL, OHYPO as AVALC from flagged where not missing(OHYPO)
    union all
    select STUDYID, USUBJID, AVISIT, 'OTACHY' as PARAMCD, 'Orthostatic Tachycardia' as PARAM, . as AVAL, OTACHY as AVALC from flagged where not missing(OTACHY);
quit;

proc export data=result(keep=STUDYID USUBJID AVISIT PARAMCD PARAM AVAL AVALC) outfile="/app/output/advs.csv" dbms=csv replace; run;
