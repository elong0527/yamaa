Following CDISC ADaM standards, use the provided AE, FA and ADSL datasets
to create an ADAE dataset with one record per analysis severity period.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ASEQ, AESEQ, AEDECOD, AESTDTC, AEENDTC, ASTDTM, AENDTM,
TRTSDTM, AESEV, AETOXGR, ASEV, ATOXGRN, TRTEMFL

AE contains one record per source event with its overall start, end and
maximum severity/grade. Copy AESEQ, AEDECOD, AESTDTC, AEENDTC, AESEV and
AETOXGR unchanged to every period for that event. Match FA to AE by
STUDYID, USUBJID and FALNKID = AELNKID. FA contains a SEV and TOXGR
finding at onset, then findings at each collected effective change time
FADTC. All change times are within the source event. There is at most
one finding per event, time and FATESTCD. These times are actual changes,
rather than visit assessments requiring an inferred onset.

Create one period per distinct event and FADTC, including onset.
Simultaneous severity and grade findings create one period. ASTDTM is
FADTC. AENDTM is the next ASTDTM for the same source event, or AEENDTC
for the last period; a missing original end means the last period is
ongoing. Periods share exactly equal end/start boundaries. ASEV takes
FASTRESC for SEV and persists until the next severity finding. ATOXGRN
similarly takes the numeric TOXGR finding, from 1 through 5. A dimension
with no earlier collected value stays empty; missing findings do not
supply a new value. AESEQ remains the source event identifier. ASEQ
numbers periods from 1 within each STUDYID and USUBJID, ordered by numeric
AESEQ and ASTDTM.

TRTSDTM is first exposure from ADSL, matched by STUDYID and USUBJID.
For an event starting at or after exposure, TRTEMFL is Y on every period.
For an event starting before exposure, compare each period at or after
exposure with the last severity and grade strictly before exposure.
Severity order is MILD < MODERATE < SEVERE. TRTEMFL first becomes Y when
either known dimension increases over its known pre-treatment value;
it stays Y on subsequent periods, even if intensity later decreases.
Earlier periods and improvement without worsening have no flag. A
missing comparison value shows no worsening for that dimension. When
original onset or exposure is missing, TRTEMFL has no value. Separate
source events with the same term never share treatment-emergent status.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
