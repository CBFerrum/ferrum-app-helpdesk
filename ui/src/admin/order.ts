// Assignment rules are tried from the highest `priority` down, and the first that fits places the ticket. A manager
// puts them in order with earlier and later; this turns the order on screen into the numbers the server reads.
// Pure, so it is tested.

/** The priorities that put `ids` in this order, the first tried first, changing as few rules as it can.
 *  `have` is what each rule has now. A rule whose number already fits is left alone. */
export function renumber(ids: string[], have: Record<string, number>): { id: string; priority: number }[] {
  const n = ids.length;
  const out: { id: string; priority: number }[] = [];
  // The first gets the highest number; ten apart so that a later insertion needs no renumbering of everything.
  const step = n * 10 <= 1000 ? 10 : 1;
  ids.forEach((id, i) => {
    const priority = Math.max(1, (n - i) * step);
    if (have[id] !== priority) out.push({ id, priority });
  });
  return out;
}

/** The ids in order, with the one at `from` moved to `to`; the same order when either is outside the list. */
export function moved<T>(list: T[], from: number, to: number): T[] {
  if (from < 0 || to < 0 || from >= list.length || to >= list.length || from === to) return list;
  const next = [...list];
  const [x] = next.splice(from, 1);
  next.splice(to, 0, x);
  return next;
}
