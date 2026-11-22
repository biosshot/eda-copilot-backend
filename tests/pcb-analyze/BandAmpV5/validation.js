// Direct BandAmpV5 validation: original design-rule thresholds in millimetres.
const a = analysis({ board });
const g = await a.geometry().name('BandAmpV5/validation')
  .rules([{ kind: 'connectivity' }, { kind: 'clearance', minMm: 0.15 }, { kind: 'edge-clearance', minMm: 0.15 }])
  .budget({ maxSeconds: 500, maxMemoryMiB: 1024 }).run();
a.plot({ title: 'BandAmpV5 geometry measurements', data: g.measurements });
a.check({ name: 'No confirmed violations (coverage may be unknown)', actual: g.violations.count(), equals: 0 });
a.check({ name: 'Expected fail: negative confirmed-report count control', actual: g.violations.items.length, equals: -1 });
a.check({ name: 'Continued after expected fail', actual: g.measurements.unit, equals: 'mm' });
a.log({ level: 'info', message: 'validation.case', data: { resultId: g.id, confirmedReports: g.violations.items.length, coverage: g.measurements.data.coverage } });
