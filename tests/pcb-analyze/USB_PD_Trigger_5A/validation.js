// Direct USB PD Trigger validation: authored transport rules, original captured copper.
const a = analysis({ board });
const g = await a.geometry().name('USB_PD_Trigger_5A/validation')
  .rules([{ kind: 'connectivity' }, { kind: 'clearance', minMm: 0.2 }, { kind: 'edge-clearance', minMm: 0.2 }])
  .budget({ maxSeconds: 500, maxMemoryMiB: 1024 }).run();
a.plot({ title: 'USB_PD_Trigger_5A geometry measurements', data: g.measurements });
a.check({ name: 'No confirmed violations (coverage may be unknown)', actual: g.violations.count(), equals: 0 });
a.check({ name: 'Expected fail: negative confirmed-report count control', actual: g.violations.items.length, equals: -1 });
a.check({ name: 'Continued after expected fail', actual: g.measurements.unit, equals: 'mm' });
a.log({ level: 'info', message: 'validation.case', data: { resultId: g.id, confirmedReports: g.violations.items.length, coverage: g.measurements.data.coverage } });
