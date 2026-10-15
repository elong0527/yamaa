import csv
from collections import defaultdict

with open('/app/input/adsl.csv', newline='') as source:
    subjects = list(csv.DictReader(source))
with open('/app/input/adae.csv', newline='') as source:
    events = list(csv.DictReader(source))
qualifying = {(e['STUDYID'], e['USUBJID']) for e in events
              if e['TRTEMFL'] == 'Y'}
groups = defaultdict(list)
for subject in subjects:
    if subject['SAFFL'] == 'Y':
        key = subject['STUDYID'], int(subject['TRT01PN']), subject['TRT01P']
        groups[key].append(subject)
rows = []
for (study, order, treatment), members in sorted(groups.items()):
    count = sum((s['STUDYID'], s['USUBJID']) in qualifying for s in members)
    for operation, result in [('COUNT', count),
                              ('PERCENT', 100 * count / len(members))]:
        rows.append([study, 'AEINC', 'SAFETY', 'ADAE', 'USUBJID',
                     "SAFFL = 'Y'", "TRTEMFL = 'Y'", 'TRT01P', order,
                     treatment, 'AEINCIDENCE', operation, result])
columns = ['STUDYID', 'ANALYSISID', 'ANALYSISSETID', 'DATASET', 'VARIABLE',
           'ANALYSISSET', 'DATASUBSET', 'GROUPINGID', 'GROUPORDER',
           'GROUPLEVEL', 'METHODID', 'OPERATIONID', 'RESULT']
with open('/app/output/results.csv', 'w', newline='') as output:
    writer = csv.writer(output)
    writer.writerow(columns)
    writer.writerows(rows)
