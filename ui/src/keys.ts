// Single-key shortcuts: when a key press is a command and when it is just someone typing. Pure,
// so it is tested without a browser (the page listens, these decide).

/** The part of an event target the rule reads, so a test needs no DOM. */
export interface Target {
  tagName?: string;
  isContentEditable?: boolean;
  closest?(selector: string): unknown;
}

/** Whether keys go to what the person is typing in or choosing from, not to the page: a field, an
 *  open menu, or anything inside a dialog (the page behind it is not theirs to command). */
export function isTyping(target: Target | null): boolean {
  if (!target) return false;
  const tag = (target.tagName ?? "").toLowerCase();
  if (tag === "input" || tag === "textarea" || tag === "select") return true;
  if (target.isContentEditable) return true;
  return !!target.closest?.("[role=textbox], [role=combobox], [role=listbox], [role=menu], [role=dialog], dialog[open]");
}

export interface KeyLike {
  key: string;
  ctrlKey?: boolean;
  metaKey?: boolean;
  altKey?: boolean;
  shiftKey?: boolean;
  defaultPrevented?: boolean;
}

/** The name a key goes by in a map of commands: "j", "?", "Enter", "ArrowDown", "Shift+x". */
export function keyName(e: KeyLike): string {
  // Enter, ArrowDown, Escape and the like are named as the browser names them.
  if (e.key.length !== 1) return e.key;
  // A symbol is the symbol typed ("?"); a letter is lower case, with Shift in front when it is held
  // (Caps Lock alone is not Shift).
  if (e.key.toLowerCase() === e.key.toUpperCase()) return e.key;
  return e.shiftKey ? `Shift+${e.key.toLowerCase()}` : e.key.toLowerCase();
}

/** The command a key press asks for, if it is one. Never with Ctrl, Cmd or Alt held, so the
 *  browser's own shortcuts keep working, and never for a press something else already handled. */
export function command<T extends string>(e: KeyLike, keys: Record<string, T>): T | null {
  if (e.ctrlKey || e.metaKey || e.altKey || e.defaultPrevented) return null;
  return keys[keyName(e)] ?? null;
}
