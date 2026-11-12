// The platform's working calendars (Admin, Calendars), by name: what an agent's shift and an agreement's targets
// count on. A person who may not list them gets `undefined` and types the name instead.

import { useQuery } from "@tanstack/react-query";
import { api } from "@ferrum/client";

/** The calendar a target counts on when its agreement names none. */
export const WORKSPACE_CALENDAR = "helpdesk_hours";

export function useCalendarNames(): string[] | undefined {
  return useQuery({
    queryKey: ["helpdesk", "calendars"],
    staleTime: 5 * 60_000,
    retry: false,
    queryFn: ({ signal }) => api.get<{ calendars: { name: string }[] }>("/api/admin/calendars", undefined, signal).then((r) => r.calendars.map((c) => c.name)),
  }).data;
}
