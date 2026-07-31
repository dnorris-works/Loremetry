import type { ReportTypeDef } from './types';

export function collectPrerequisites(
  reportId: string,
  reportTypes: ReportTypeDef[],
): string[] {
  const byId = new Map(reportTypes.map(r => [r.id, r]));
  const result: string[] = [];
  const visited = new Set<string>();

  function walk(id: string): void {
    const def = byId.get(id);
    if (!def) return;
    for (const dep of def.depends_on) {
      if (dep === reportId || visited.has(dep)) continue;
      visited.add(dep);
      result.push(dep);
      walk(dep);
    }
  }

  if (byId.get(reportId)) walk(reportId);
  return result;
}

export function buildRunQueue(
  primaryIds: string[],
  reportTypes: ReportTypeDef[],
  shouldRunDep: (depId: string) => boolean,
): string[] {
  const runSet = new Set<string>();
  for (const id of primaryIds) {
    runSet.add(id);
    for (const dep of collectPrerequisites(id, reportTypes)) {
      if (shouldRunDep(dep)) runSet.add(dep);
    }
  }
  return [...runSet];
}
