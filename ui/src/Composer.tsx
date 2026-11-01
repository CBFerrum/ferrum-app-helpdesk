// The box under a ticket's feed: a reply to the customer, or a note for the team. `/` finds a saved
// reply, files are dropped, pasted or attached, Ctrl or Cmd with Enter sends. What is typed is kept
// for this ticket until it is sent, and the tab warns before it is closed on unsent words.

import { type ChangeEvent, type ClipboardEvent, type DragEvent, type KeyboardEvent, type RefObject, useState } from "react";
import { api, errorMessage } from "@ferrum/client";
import { Alert, Button, Icon, toast } from "@ferrum/kit";
import { fileSize } from "./feed";
import { useDraft, useLeaveWarning } from "./hooks";
import { ReplyPicker } from "./ReplyPicker";
import { type FillWith, insertAt, leftover } from "./replies";

export type Mode = "reply" | "note";

export interface Uploaded {
  id: string;
  file_name: string;
  size: number;
}

export interface Outgoing {
  internal: boolean;
  body: string;
  files: Uploaded[];
}

/** What the server takes in one message (Helpdesk's `outgoing.rs`): what one email can carry. */
const MAX_FILES = 10;
const MAX_BYTES = 20 * 1024 * 1024;

async function upload(file: File): Promise<Uploaded> {
  const body = new FormData();
  body.append("file", file);
  const stored = await api.post<Uploaded>("/api/files", body);
  return { id: stored.id, file_name: stored.file_name, size: stored.size };
}

export function Composer({
  ticket,
  mode,
  onMode,
  inputRef,
  to,
  assignee,
  about,
  replies,
  conflict,
  onSend,
}: {
  ticket: string;
  mode: Mode;
  onMode: (m: Mode) => void;
  inputRef: RefObject<HTMLTextAreaElement | null>;
  /** Who a reply is for; no address means it stays in the conversation and the portal. */
  to: { name: string; email: string | null };
  /** Who has the ticket, as a sentence ("Priya has this ticket"), or null when it is the agent's own or nobody's. */
  assignee: string | null;
  about: FillWith;
  /** Whether the workspace has saved replies at all. */
  replies: boolean;
  /** Set when the ticket changed under the agent while they were writing. */
  conflict: string | null;
  /** Resolves true when it was sent, which empties the box. */
  onSend: (m: Outgoing) => Promise<boolean>;
}) {
  const [reply, setReply] = useDraft(`helpdesk:draft:${ticket}:reply`);
  const [note, setNote] = useDraft(`helpdesk:draft:${ticket}:note`);
  const [files, setFiles] = useState<Record<Mode, Uploaded[]>>({ reply: [], note: [] });
  const [busy, setBusy] = useState(false);
  const [uploading, setUploading] = useState(0);
  const [picker, setPicker] = useState(false);
  const [dropping, setDropping] = useState(false);

  const body = mode === "reply" ? reply : note;
  const setBody = mode === "reply" ? setReply : setNote;
  const mine = files[mode];
  const open = leftover(body);
  const ready = !!body.trim() && !open.length && !uploading && !busy;
  useLeaveWarning(!!reply.trim() || !!note.trim());

  const add = async (picked: File[]) => {
    const room = MAX_FILES - mine.length;
    const total = mine.reduce((n, f) => n + f.size, 0);
    if (picked.length > room) toast(`A message carries at most ${MAX_FILES} files.`, "error");
    let used = total;
    for (const file of picked.slice(0, Math.max(0, room))) {
      if (used + file.size > MAX_BYTES) {
        toast(`${file.name} would take the files past ${MAX_BYTES / 1024 / 1024} MB, the most one email carries.`, "error");
        continue;
      }
      used += file.size;
      setUploading((n) => n + 1);
      try {
        const stored = await upload(file);
        setFiles((all) => ({ ...all, [mode]: [...all[mode], stored] }));
      } catch (err) {
        toast(`${file.name}: ${errorMessage(err)}`, "error");
      } finally {
        setUploading((n) => n - 1);
      }
    }
  };
  const drop = (f: Uploaded) => {
    setFiles((all) => ({ ...all, [mode]: all[mode].filter((x) => x.id !== f.id) }));
    // Tidying the upload away is best effort: a file left behind belongs to nothing and shows nowhere.
    void api.del(`/api/files/${f.id}`).catch(() => {});
  };

  const send = async () => {
    if (!ready) return;
    setBusy(true);
    const sent = await onSend({ internal: mode === "note", body: body.trim(), files: mine });
    setBusy(false);
    if (!sent) return;
    setBody("");
    setFiles((all) => ({ ...all, [mode]: [] }));
    inputRef.current?.focus();
  };

  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      void send();
    } else if (e.key === "/" && replies && mode === "reply" && !e.metaKey && !e.ctrlKey && !e.altKey) {
      // A slash that starts a line asks for a saved reply; anywhere else it is a slash.
      const t = e.currentTarget;
      const before = t.value.slice(0, t.selectionStart);
      if (before === "" || /\s$/.test(before)) {
        e.preventDefault();
        setPicker(true);
      }
    } else if (e.key === "Escape") {
      e.currentTarget.blur();
    }
  };
  const onPaste = (e: ClipboardEvent<HTMLTextAreaElement>) => {
    if (e.clipboardData.files.length) {
      e.preventDefault();
      void add([...e.clipboardData.files]);
    }
  };
  const onDrop = (e: DragEvent) => {
    e.preventDefault();
    setDropping(false);
    void add([...e.dataTransfer.files]);
  };
  const choose = (e: ChangeEvent<HTMLInputElement>) => {
    void add([...(e.target.files ?? [])]);
    e.target.value = "";
  };
  const pasted = (text: string) => {
    const el = inputRef.current;
    const next = insertAt(body, el?.selectionStart ?? body.length, el?.selectionEnd ?? body.length, text);
    setBody(next.text);
    setPicker(false);
    requestAnimationFrame(() => {
      el?.focus();
      el?.setSelectionRange(next.caret, next.caret);
    });
  };

  return (
    <section
      className="hd-write"
      data-mode={mode}
      data-dropping={dropping || undefined}
      aria-label="Write to the ticket"
      onDragOver={(e) => {
        if (e.dataTransfer.types.includes("Files")) {
          e.preventDefault();
          setDropping(true);
        }
      }}
      onDragLeave={() => setDropping(false)}
      onDrop={onDrop}
    >
      <div className="hd-write-tabs" role="tablist" aria-label="What to write">
        <button type="button" role="tab" aria-selected={mode === "reply"} onClick={() => onMode("reply")}>
          Reply
        </button>
        <button type="button" role="tab" aria-selected={mode === "note"} onClick={() => onMode("note")}>
          Internal note
        </button>
        <span className="spacer" />
        <span className="faint hd-write-to">
          {mode === "note" ? "Only your team sees this" : to.email ? `To ${to.name} <${to.email}>` : "No email address on this ticket"}
        </span>
      </div>
      {conflict && <Alert tone="warning">{conflict}</Alert>}
      <textarea
        ref={inputRef}
        id="hd-write-text"
        className="textarea hd-write-text"
        rows={4}
        aria-label={mode === "reply" ? "Reply to the customer" : "Internal note"}
        aria-describedby="hd-write-help"
        placeholder={mode === "reply" ? (replies ? "Write a reply. Press / for a saved reply." : "Write a reply") : "Write a note for your team"}
        value={body}
        maxLength={20000}
        onChange={(e) => setBody(e.target.value)}
        onKeyDown={onKeyDown}
        onPaste={onPaste}
      />
      <div id="hd-write-help" className="hd-write-help faint">
        {mode === "reply" && !to.email && "The reply is kept in the conversation, and the customer can read it in the portal. "}
        {mode === "reply" && assignee && `${assignee} Sending takes it over. `}
        Markdown works. Drop or paste files here.
      </div>
      {open.length > 0 && (
        <p className="hd-warn" role="status">
          {open.map((o) => `{{${o}}}`).join(", ")} still to fill in before this can be sent.
        </p>
      )}
      {(mine.length > 0 || uploading > 0) && (
        <ul className="hd-files" aria-label="Files to send">
          {mine.map((f) => (
            <li key={f.id} className="hd-file">
              <Icon name="file" size={14} />
              <span className="hd-file-name">{f.file_name}</span>
              <span className="faint">{fileSize(f.size)}</span>
              <button type="button" className="icon-btn icon-btn-sm" aria-label={`Remove ${f.file_name}`} onClick={() => drop(f)}>
                <Icon name="x" size={12} />
              </button>
            </li>
          ))}
          {uploading > 0 && (
            <li className="hd-file" aria-live="polite">
              <span className="spinner spinner-sm" aria-hidden="true" />
              Uploading…
            </li>
          )}
        </ul>
      )}
      <div className="hd-write-foot">
        {replies && mode === "reply" && (
          <button
            type="button"
            className="btn btn-ghost btn-sm"
            aria-haspopup="dialog"
            aria-expanded={picker}
            onPointerDown={(e) => e.stopPropagation()}
            onClick={() => setPicker((p) => !p)}
          >
            Saved replies
            <kbd className="kbd">/</kbd>
          </button>
        )}
        <label className="btn btn-ghost btn-sm">
          <Icon name="upload" />
          Attach
          <input type="file" multiple className="sr-only" onChange={choose} />
        </label>
        <span className="spacer" />
        <span className="faint hd-hint">Ctrl or ⌘ + Enter</span>
        <Button variant="primary" loading={busy} disabled={!ready} onClick={() => void send()}>
          {mode === "reply" ? "Send reply" : "Add note"}
        </Button>
      </div>
      {picker && <ReplyPicker about={about} draft={body} onPick={pasted} onClose={() => setPicker(false)} />}
    </section>
  );
}
