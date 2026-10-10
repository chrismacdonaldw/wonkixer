export const FORBIDDEN_NAME_CHARS = /[\\/:*?"<>|]/;
export const PRESETS_FOLDER = "%APPDATA%\\com.chrismacdonaldw.wonkixer\\presets";

export type DialogMode = "save" | "rename";
export type NoteTone = "muted" | "warn" | "bad";

export interface NameCheck {
  trimmed: string;
  blocked: boolean;
  replaces: boolean;
  note: string;
  tone: NoteTone;
  confirmText: string;
  invalid: boolean;
}

export interface NameContext {
  builtin: string[];
  user: string[];
  mode: DialogMode;
  from?: string;
}

export function checkPresetName(name: string, context: NameContext): NameCheck {
  const trimmed = name.trim();
  const builtinClash = context.builtin.includes(trimmed);
  const userClash = context.user.includes(trimmed) && !(context.mode === "rename" && context.from === trimmed);
  const badChars = FORBIDDEN_NAME_CHARS.test(trimmed);
  const blocked = !trimmed || builtinClash || badChars;
  const note = !trimmed
    ? "Give it a name."
    : builtinClash
      ? "That name belongs to a built-in preset. Pick another."
      : badChars
        ? 'Names cannot contain \\ / : * ? " < > |'
        : userClash
          ? "You already have a preset with this name. Saving replaces it."
          : context.mode === "rename"
            ? "Renames the file. The settings stay as they are."
            : "Saves every effect, every value and which effects are on.";
  const invalid = blocked && trimmed.length > 0;
  return {
    trimmed,
    blocked,
    replaces: userClash,
    note,
    tone: invalid ? "bad" : userClash ? "warn" : "muted",
    confirmText: context.mode === "rename" ? "RENAME" : userClash ? "REPLACE" : "SAVE",
    invalid,
  };
}

export function presetFilePath(name: string): string {
  return `${PRESETS_FOLDER}\\${name || "…"}.json`;
}

export function duplicateName(base: string, existing: string[]): string {
  let candidate = `${base} copy`;
  let counter = 2;
  while (existing.includes(candidate)) {
    candidate = `${base} copy ${counter}`;
    counter += 1;
  }
  return candidate;
}

export function suggestedSaveName(activeName: string | null): string {
  return `${activeName ?? "My preset"} (mine)`;
}
