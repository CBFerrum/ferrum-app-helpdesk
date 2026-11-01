import { describe, expect, it } from "vitest";
import { command, isTyping, keyName } from "./keys";

const closest = (matching: boolean) => ({ tagName: "DIV", closest: () => (matching ? {} : null) });

describe("when keys are typing", () => {
  it("is so in a field, a menu list or a dialog", () => {
    expect(isTyping({ tagName: "TEXTAREA" })).toBe(true);
    expect(isTyping({ tagName: "input" })).toBe(true);
    expect(isTyping({ tagName: "SELECT" })).toBe(true);
    expect(isTyping({ tagName: "DIV", isContentEditable: true })).toBe(true);
    expect(isTyping(closest(true))).toBe(true);
  });

  it("is not so on the page itself", () => {
    expect(isTyping(null)).toBe(false);
    expect(isTyping({ tagName: "BODY" })).toBe(false);
    expect(isTyping(closest(false))).toBe(false);
    expect(isTyping({ tagName: "A" })).toBe(false);
  });
});

describe("naming keys", () => {
  it("names letters in lower case, with Shift only when it is held", () => {
    expect(keyName({ key: "j" })).toBe("j");
    expect(keyName({ key: "J" })).toBe("j");
    expect(keyName({ key: "X", shiftKey: true })).toBe("Shift+x");
    expect(keyName({ key: "x", shiftKey: true })).toBe("Shift+x");
  });

  it("names symbols and special keys as they come", () => {
    expect(keyName({ key: "?", shiftKey: true })).toBe("?");
    expect(keyName({ key: "/" })).toBe("/");
    expect(keyName({ key: "Enter" })).toBe("Enter");
    expect(keyName({ key: "ArrowDown" })).toBe("ArrowDown");
  });
});

describe("commands", () => {
  const keys = { j: "next", k: "prev", e: "resolve", "Shift+x": "range", "?": "help", ArrowDown: "next" } as const;

  it("find the command a key stands for", () => {
    expect(command({ key: "j" }, keys)).toBe("next");
    expect(command({ key: "ArrowDown" }, keys)).toBe("next");
    expect(command({ key: "?", shiftKey: true }, keys)).toBe("help");
    expect(command({ key: "X", shiftKey: true }, keys)).toBe("range");
    expect(command({ key: "z" }, keys)).toBeNull();
  });

  it("leave the browser's own shortcuts alone", () => {
    expect(command({ key: "e", ctrlKey: true }, keys)).toBeNull();
    expect(command({ key: "e", metaKey: true }, keys)).toBeNull();
    expect(command({ key: "e", altKey: true }, keys)).toBeNull();
  });

  it("leave a press that something else already handled", () => {
    expect(command({ key: "e", defaultPrevented: true }, keys)).toBeNull();
  });
});
