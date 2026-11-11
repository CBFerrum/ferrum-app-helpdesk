// Colours for the stored ids of a ticket's status, priority and service-level state (the option
// lists in packages/helpdesk/entities/support.yaml). Never derived from the words shown.
import type { Tone } from "@ferrum/kit";

export const STATUS_TONES: Readonly<Record<string, Tone>> = { Open: "blue", Replied: "purple", "On Hold": "amber", Resolved: "green", Closed: "neutral" };
export const PRIORITY_TONES: Readonly<Record<string, Tone>> = { Low: "neutral", High: "amber", Urgent: "red" };
export const SLA_TONES: Readonly<Record<string, Tone>> = { "On Track": "neutral", "At Risk": "amber", Breached: "red", Fulfilled: "green" };
