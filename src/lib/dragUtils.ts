/** Shared drag-and-drop position math and utility functions. */

/**
 * Compute per-item translateY offsets for CSS preview animation.
 * naturalOrder and previewOrder must contain the same set of IDs.
 */
export function computePreviewTranslates(
  naturalOrder: string[],
  previewOrder: string[],
  heights: Map<string, number>,
  fallback: number
): Map<string, number> {
  let nat = 0;
  const natPos = new Map<string, number>();
  for (const id of naturalOrder) {
    natPos.set(id, nat);
    nat += heights.get(id) ?? fallback;
  }
  let pre = 0;
  const prePos = new Map<string, number>();
  for (const id of previewOrder) {
    prePos.set(id, pre);
    pre += heights.get(id) ?? fallback;
  }
  const offsets = new Map<string, number>();
  for (const id of naturalOrder) {
    offsets.set(id, (prePos.get(id) ?? 0) - (natPos.get(id) ?? 0));
  }
  return offsets;
}

/** Deep-search for a row by ID (searches alternatives recursively). */
export function findRowByIdDeep<T extends { id: string; alternatives?: T[] }>(
  rows: T[],
  id: string
): T | undefined {
  for (const r of rows) {
    if (r.id === id) return r;
    if (r.alternatives?.length) {
      const found = findRowByIdDeep(r.alternatives, id);
      if (found) return found;
    }
  }
  return undefined;
}

/** Return a new rows array with targetId's alternatives replaced by newAlts (recursive). */
export function updateAltsDeep<T extends { id: string; alternatives?: T[] }>(
  rows: T[],
  targetId: string,
  newAlts: T[]
): T[] {
  return rows.map(r => {
    if (r.id === targetId) return { ...r, alternatives: newAlts };
    if (!r.alternatives?.length) return r;
    const updated = updateAltsDeep(r.alternatives, targetId, newAlts);
    return updated === r.alternatives ? r : { ...r, alternatives: updated };
  });
}
