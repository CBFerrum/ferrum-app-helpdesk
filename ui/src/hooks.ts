// Small things both screens lean on: the viewport, what the browser remembers, live events that
// arrive in bursts, drafts that survive a page change, and the people agents know each other by.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { usePeople, type Person } from "@ferrum/records";

/** Whether a media query matches now, and as the window changes. */
export function useMedia(query: string): boolean {
  const [on, setOn] = useState(() => typeof matchMedia === "function" && matchMedia(query).matches);
  useEffect(() => {
    if (typeof matchMedia !== "function") return;
    const m = matchMedia(query);
    const change = () => setOn(m.matches);
    change();
    m.addEventListener("change", change);
    return () => m.removeEventListener("change", change);
  }, [query]);
  return on;
}

/** A choice kept in the browser between visits. A browser that refuses storage forgets it with the page. */
export function useRemembered<T extends string | boolean>(key: string, fallback: T): [T, (v: T) => void] {
  const [value, setValue] = useState<T>(() => {
    try {
      const stored = localStorage.getItem(key);
      return stored === null ? fallback : (JSON.parse(stored) as T);
    } catch {
      return fallback;
    }
  });
  const remember = useCallback(
    (v: T) => {
      setValue(v);
      try {
        localStorage.setItem(key, JSON.stringify(v));
      } catch {
        /* a private window: the choice lasts until the page closes */
      }
    },
    [key],
  );
  return [value, remember];
}

/** A function that runs once things have been quiet for `ms`: a busy desk raises a live event for
 *  every change, and the screen only needs to look again when they stop. */
export function useQuiet(fn: () => void, ms: number): () => void {
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const latest = useRef(fn);
  latest.current = fn;
  useEffect(() => () => clearTimeout(timer.current), []);
  return useCallback(() => {
    clearTimeout(timer.current);
    timer.current = setTimeout(() => latest.current(), ms);
  }, [ms]);
}

/** Text typed and not yet sent, kept for this tab so that going to another ticket and back loses nothing. */
export function useDraft(key: string): [string, (v: string) => void] {
  const [text, setText] = useState(() => {
    try {
      return sessionStorage.getItem(key) ?? "";
    } catch {
      return "";
    }
  });
  const update = useCallback(
    (v: string) => {
      setText(v);
      try {
        if (v) sessionStorage.setItem(key, v);
        else sessionStorage.removeItem(key);
      } catch {
        /* storage refused: the draft lasts as long as the page */
      }
    },
    [key],
  );
  return [text, update];
}

/** Asks the browser to warn before the tab closes or reloads while `dirty`. */
export function useLeaveWarning(dirty: boolean) {
  useEffect(() => {
    if (!dirty) return;
    const warn = (e: BeforeUnloadEvent) => e.preventDefault();
    window.addEventListener("beforeunload", warn);
    return () => window.removeEventListener("beforeunload", warn);
  }, [dirty]);
}

/** The workspace's people: the name an address goes by, and the list for choosing from. */
export function useNames(): { name: (email: string) => string; people: Person[] } {
  const users = usePeople().data?.users;
  return useMemo(() => {
    const by = new Map((users ?? []).map((p) => [p.email.toLowerCase(), p.full_name]));
    return { name: (email: string) => by.get(email.toLowerCase()) || email, people: users ?? [] };
  }, [users]);
}

/** The user's preference for reduced motion, for the few places that move on their own. */
export function prefersReducedMotion(): boolean {
  return typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/** The time, kept fresh: "waiting 3 h" and "due in 20 min" move on while a screen stays open. */
export function useNow(every = 60_000): number {
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), every);
    return () => clearInterval(t);
  }, [every]);
  return now;
}
