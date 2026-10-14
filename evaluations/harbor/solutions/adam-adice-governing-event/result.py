import csv
from collections import defaultdict

with open('/app/input/ice_raw.csv', newline='') as source:
    records = list(csv.DictReader(source))
priority = {'ICE04': 1, 'ICE05': 1, 'ICE01': 2, 'ICE02': 3, 'ICE03': 4}
groups = defaultdict(list)
for record in records:
    groups[record['STUDYID'], record['USUBJID']].append(record)
rows = []
for (study, subject), events in sorted(groups.items()):
    ordered = sorted(events, key=lambda r: (priority[r['ICETYPE']],
                                           r['ICEDTC'], int(r['ICESEQ'])))
    winner = ordered[0]
    for record in sorted(events, key=lambda r: int(r['ICESEQ'])):
        rank = ordered.index(record) + 1
        rows.append([study, subject, record['ICESEQ'], record['ICETYPE'],
                     record['ICEDTC'], priority[record['ICETYPE']], rank,
                     'Y' if rank == 1 else '', '' if rank == 1 else 'Y',
                     winner['ICETYPE'], winner['ICEDTC'], winner['ICESEQ']])
columns = ['STUDYID', 'USUBJID', 'ASEQ', 'ADECOD1', 'ASTDT', 'ICEPRIO',
           'ICERANK', 'GOVFL', 'SUPERFL', 'GOVTYPE', 'GOVDT', 'GOVSEQ']
with open('/app/output/adice.csv', 'w', newline='') as output:
    writer = csv.writer(output)
    writer.writerow(columns)
    writer.writerows(rows)
