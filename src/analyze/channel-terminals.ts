export interface PhysicalTerminal { id: string; sourcePath: string; layer: string; }
export interface PhysicalPort { name: string; positive: string; negative: string; }
export interface ChannelTerminalRequest { signals: PhysicalTerminal[]; references: PhysicalTerminal[]; ports: PhysicalPort[]; }
export interface LegacyChannelContact { sourcePath: string; layer: string; }

const invalid = (message: string): never => { throw Object.assign(new Error(`CHANNEL_TERMINALS: ${message}`), { code: 'CHANNEL_TERMINALS_INVALID' }); };

/** Validate physical IDs and explicit port polarity without implying electrical bonds. */
export function validateChannelTerminalRequest(request: ChannelTerminalRequest): ChannelTerminalRequest {
  if (!request || !Array.isArray(request.signals) || request.signals.length === 0) invalid('At least one signal terminal is required');
  if (!Array.isArray(request.references) || request.references.length === 0) invalid('At least one reference terminal is required');
  const terminals = [...request.signals, ...request.references];
  const ids = new Set<string>();
  for (const t of terminals) {
    if (!t || typeof t.id !== 'string' || !t.id.trim() || typeof t.sourcePath !== 'string' || !t.sourcePath.trim() || typeof t.layer !== 'string' || !t.layer.trim()) invalid('Each terminal requires nonempty id, sourcePath, and layer');
    if (ids.has(t.id)) invalid(`Duplicate physical terminal id: ${t.id}`);
    ids.add(t.id);
  }
  if (!Array.isArray(request.ports) || request.ports.length === 0) invalid('At least one explicit port is required');
  const names = new Set<string>();
  for (const p of request.ports) {
    if (!p || typeof p.name !== 'string' || !p.name.trim() || names.has(p.name)) invalid('Port names must be nonempty and unique');
    if (!ids.has(p.positive) || !ids.has(p.negative) || p.positive === p.negative) invalid(`Port ${p.name} must reference two distinct declared terminals`);
    names.add(p.name);
  }
  return request;
}

/** Adapt the existing two-ended signal/return request to the typed contract. */
export function legacyChannelTerminalRequest(contacts: LegacyChannelContact[]): ChannelTerminalRequest {
  if (!Array.isArray(contacts) || contacts.length !== 4) invalid('Legacy planner requires exactly four contacts: signal-in, signal-out, return-in, return-out');
  const terminals = contacts.map((c, i) => {
    if (!c || typeof c.sourcePath !== 'string' || !c.sourcePath.trim() || typeof c.layer !== 'string' || !c.layer.trim()) invalid(`Legacy contact ${i} requires sourcePath and layer`);
    return { id: `legacy-${i}`, sourcePath: c.sourcePath, layer: c.layer };
  });
  const signals = terminals.slice(0, 2);
  const references = terminals.slice(2);
  const byContact = new Map<string, PhysicalTerminal>();
  const unique = (group: PhysicalTerminal[]) => group.filter(terminal => {
    const key = JSON.stringify([terminal.sourcePath, terminal.layer]);
    if (byContact.has(key)) return false;
    byContact.set(key, terminal);
    return true;
  });
  const uniqueSignals = unique(signals), uniqueReferences = unique(references);
  const id = (terminal: PhysicalTerminal) => byContact.get(JSON.stringify([terminal.sourcePath, terminal.layer]))!.id;
  return validateChannelTerminalRequest({ signals: uniqueSignals, references: uniqueReferences, ports: [
    { name: 'input', positive: id(signals[0]), negative: id(references[0]) },
    { name: 'output', positive: id(signals[1]), negative: id(references[1]) },
  ] });
}

/** Validate planner limits and snapshot the declared contacts and oriented pairs. */
export function normalizePlannerTerminals(input: LegacyChannelContact[] | ChannelTerminalRequest) {
  const request = validateChannelTerminalRequest(Array.isArray(input) ? legacyChannelTerminalRequest(input) : input);
  if (request.ports.length > 16) invalid('Current composition supports at most 16 external ports');

  const contacts = [...request.signals, ...request.references].map(({ id, sourcePath, layer }) => ({ id, sourcePath, layer }));
  const pairs = new Set<string>(), used = new Set<string>();
  for (const { name, positive, negative } of request.ports) {
    const key = JSON.stringify([positive, negative]);
    if (pairs.has(key)) invalid(`Duplicate oriented physical port pair: ${name}`);
    pairs.add(key);
    used.add(positive);
    used.add(negative);
  }
  const sources = new Set<string>();
  for (const { id, sourcePath, layer } of contacts) {
    const key = JSON.stringify([sourcePath, layer]);
    if (sources.has(key)) invalid(`Multiple terminal IDs select the same physical contact: ${id}`);
    if (!used.has(id)) invalid(`Declared terminal is unused by any port: ${id}`);
    sources.add(key);
  }
  return {
    contacts,
    signalIds: new Set(request.signals.map(t => t.id)),
    ports: request.ports.map(({ name, positive, negative }) => ({ name, positive, negative })),
  };
}
