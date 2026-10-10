import { create, element } from "./dom.ts";
import type { EngineApi, PresetRef } from "./engine.ts";
import { describeError } from "./errors.ts";
import { checkPresetName, duplicateName, presetFilePath, suggestedSaveName, type DialogMode } from "./presets-logic.ts";
import type { Session } from "./session.ts";
import { canCompare, canSave, currentDocument, isEdited, presetName, type SessionState } from "./session-state.ts";

interface Dialog {
  mode: DialogMode;
  from?: string;
}

const BUILTIN_NOTE = "Built-in presets are read-only, so Save as keeps your changes. Your current settings come back on launch either way.";
const USER_NOTE = "One .json file per preset. Your current settings come back on launch even if you never save.";

export function initPresetBar(session: Session, engine: EngineApi, doc: Document): void {
  const edited = element<HTMLElement>("#preset-edited", doc);
  const open = element<HTMLButtonElement>("#preset-open", doc);
  const name = element<HTMLElement>("#preset-name", doc);
  const undo = element<HTMLButtonElement>("#undo", doc);
  const redo = element<HTMLButtonElement>("#redo", doc);
  const compare = element<HTMLButtonElement>("#compare", doc);
  const quickSave = element<HTMLButtonElement>("#quick-save", doc);
  const dialog = element<HTMLElement>("#presets-dialog", doc);
  const builtinList = element<HTMLElement>("#presets-builtin", doc);
  const userList = element<HTMLElement>("#presets-user", doc);
  const userCount = element<HTMLElement>("#presets-user-count", doc);
  const empty = element<HTMLElement>("#presets-empty", doc);
  const save = element<HTMLButtonElement>("#preset-save", doc);
  const saveAs = element<HTMLButtonElement>("#preset-save-as", doc);
  const revert = element<HTMLButtonElement>("#preset-revert", doc);
  const importButton = element<HTMLButtonElement>("#preset-import", doc);
  const exportButton = element<HTMLButtonElement>("#preset-export", doc);
  const note = element<HTMLElement>("#presets-note", doc);
  const folder = element<HTMLButtonElement>("#presets-folder", doc);
  const overlay = element<HTMLElement>("#name-dialog", doc);
  const dialogTitle = element<HTMLElement>("#dialog-title", doc);
  const nameInput = element<HTMLInputElement>("#preset-name-input", doc);
  const dialogNote = element<HTMLElement>("#dialog-note", doc);
  const dialogPath = element<HTMLElement>("#dialog-path", doc);
  const cancel = element<HTMLButtonElement>("#dialog-cancel", doc);
  const confirm = element<HTMLButtonElement>("#dialog-confirm", doc);

  let listOpen = false;
  let message = "";
  let current: Dialog | null = null;

  const setNote = (text: string): void => {
    message = text;
    render(session.store.get());
  };

  const attempt = async (task: () => Promise<void>): Promise<void> => {
    try {
      await task();
    } catch (error) {
      setNote(describeError(error));
    }
  };

  const refreshCatalog = async (): Promise<void> => {
    session.setCatalog(await engine.listPresets());
  };

  const setListOpen = (value: boolean): void => {
    listOpen = value;
    if (value) {
      message = "";
    }
    render(session.store.get());
  };

  const pick = (ref: PresetRef): Promise<void> =>
    attempt(async () => {
      const doc = await engine.loadPreset(ref);
      session.applyDocument(doc, ref);
      setListOpen(false);
    });

  const saveActive = (): Promise<void> =>
    attempt(async () => {
      const state = session.store.get();
      const active = state.activePreset;
      if (!active || active.kind !== "user") {
        return;
      }
      await engine.saveUserPreset(active.name, currentDocument(state));
      session.markSaved(active);
      await refreshCatalog();
      setNote(`Saved ${active.name}.`);
    });

  const openDialog = (next: Dialog, initial: string): void => {
    current = next;
    listOpen = false;
    nameInput.value = initial;
    render(session.store.get());
    nameInput.focus();
    nameInput.select();
  };

  const closeDialog = (): void => {
    current = null;
    render(session.store.get());
  };

  const openSaveAs = (): void => {
    openDialog({ mode: "save" }, suggestedSaveName(session.store.get().activePreset?.name ?? null));
  };

  const confirmDialog = (): Promise<void> =>
    attempt(async () => {
      const active = current;
      if (!active) {
        return;
      }
      const state = session.store.get();
      const check = checkPresetName(nameInput.value, nameContext(state, active));
      if (check.blocked) {
        return;
      }
      if (active.mode === "rename" && active.from !== undefined) {
        const from = active.from;
        await engine.renameUserPreset(from, check.trimmed);
        session.renamePreset(from, check.trimmed);
        await refreshCatalog();
        current = null;
        listOpen = true;
        setNote(`Renamed to ${check.trimmed}.`);
        return;
      }
      await engine.saveUserPreset(check.trimmed, currentDocument(state));
      session.markSaved({ kind: "user", name: check.trimmed });
      await refreshCatalog();
      current = null;
      setListOpen(false);
    });

  open.addEventListener("click", () => setListOpen(!listOpen));
  undo.addEventListener("click", () => session.undo());
  redo.addEventListener("click", () => session.redo());
  compare.addEventListener("click", () => session.toggleCompare());
  quickSave.addEventListener("click", () => {
    const state = session.store.get();
    if (canSave(state, isEdited(session.schema, state))) {
      void saveActive();
    } else {
      openSaveAs();
    }
  });
  save.addEventListener("click", () => void saveActive());
  saveAs.addEventListener("click", openSaveAs);
  revert.addEventListener("click", () => {
    const state = session.store.get();
    if (state.activeDocument) {
      session.applyDocument(state.activeDocument, state.activePreset);
    }
  });
  importButton.addEventListener("click", () =>
    void attempt(async () => {
      const imported = await engine.importPreset();
      if (imported) {
        await refreshCatalog();
        setNote(`Imported ${imported.name}.`);
      }
    }),
  );
  exportButton.addEventListener("click", () =>
    void attempt(async () => {
      const state = session.store.get();
      const path = await engine.exportPreset(presetName(state), currentDocument(state));
      if (path) {
        setNote(`Exported to ${path}.`);
      }
    }),
  );
  folder.addEventListener("click", () => void attempt(() => engine.openPresetsFolder()));
  cancel.addEventListener("click", closeDialog);
  confirm.addEventListener("click", () => void confirmDialog());
  nameInput.addEventListener("input", () => render(session.store.get()));
  nameInput.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      void confirmDialog();
    } else if (event.key === "Escape") {
      event.preventDefault();
      closeDialog();
    }
  });

  doc.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && listOpen) {
      setListOpen(false);
    }
  });
  doc.addEventListener("pointerdown", (event) => {
    const target = event.target as Node;
    if (listOpen && !dialog.contains(target) && !open.contains(target)) {
      setListOpen(false);
    }
  });

  const presetRow = (ref: PresetRef, state: SessionState, editedNow: boolean): HTMLButtonElement => {
    const active = state.activePreset?.kind === ref.kind && state.activePreset.name === ref.name;
    const row = create(doc, "button", "preset-row");
    row.type = "button";
    row.setAttribute("aria-current", String(active));
    row.classList.toggle("is-active", active);
    const mark = create(doc, "span", "preset-mark", active ? "✓" : "");
    mark.setAttribute("aria-hidden", "true");
    row.append(mark, create(doc, "span", "preset-row-name", ref.name), create(doc, "span", "preset-tag", active && editedNow ? "EDITED" : ""));
    row.addEventListener("click", () => void pick(ref));
    return row;
  };

  const iconButton = (className: string, label: string, svg: string, onClick: () => void): HTMLButtonElement => {
    const button = create(doc, "button", `preset-icon ${className}`);
    button.type = "button";
    button.setAttribute("aria-label", label);
    button.innerHTML = svg;
    button.addEventListener("click", onClick);
    return button;
  };

  const renderLists = (state: SessionState, editedNow: boolean): void => {
    builtinList.replaceChildren(...state.catalog.builtin.map((builtin) => presetRow({ kind: "builtin", name: builtin }, state, editedNow)));
    const names = state.catalog.user.map((user) => user.name);
    userList.replaceChildren(
      ...state.catalog.user.map((user) => {
        const ref: PresetRef = { kind: "user", name: user.name };
        const wrap = create(doc, "div", "preset-user");
        wrap.classList.toggle("is-active", state.activePreset?.kind === "user" && state.activePreset.name === user.name);
        wrap.append(
          presetRow(ref, state, editedNow),
          iconButton("preset-rename", `Rename ${user.name}`, RENAME_ICON, () => openDialog({ mode: "rename", from: user.name }, user.name)),
          iconButton("preset-duplicate", `Duplicate ${user.name}`, DUPLICATE_ICON, () =>
            void attempt(async () => {
              const copy = duplicateName(user.name, names);
              await engine.saveUserPreset(copy, await engine.loadPreset(ref));
              await refreshCatalog();
              setNote(`Duplicated as ${copy}.`);
            }),
          ),
          iconButton("preset-delete", `Delete ${user.name}`, DELETE_ICON, () =>
            void attempt(async () => {
              await engine.deleteUserPreset(user.name);
              session.forgetPreset(user.name);
              await refreshCatalog();
              setNote(`Deleted ${user.name}. The file goes to the Recycle Bin, so it can be restored.`);
            }),
          ),
        );
        return wrap;
      }),
    );
    userCount.textContent = String(state.catalog.user.length);
    empty.hidden = state.catalog.user.length > 0;
  };

  const render = (state: SessionState): void => {
    const editedNow = isEdited(session.schema, state);
    const saveable = canSave(state, editedNow);
    const comparable = canCompare(state, editedNow);
    const comparing = state.compare !== null;
    edited.hidden = !editedNow;
    name.textContent = presetName(state);
    open.setAttribute("aria-expanded", String(listOpen));
    open.classList.toggle("is-open", listOpen);
    undo.disabled = state.undoDepth === 0;
    redo.disabled = state.redoDepth === 0;
    compare.disabled = !comparable;
    compare.setAttribute("aria-pressed", String(comparing));
    compare.classList.toggle("is-on", comparing);
    quickSave.textContent = saveable ? "SAVE" : "SAVE AS…";
    quickSave.classList.toggle("is-edited", editedNow);
    dialog.hidden = !listOpen;
    if (listOpen) {
      renderLists(state, editedNow);
    }
    save.disabled = !saveable;
    revert.hidden = !editedNow;
    note.textContent = message || (state.activePreset?.kind === "builtin" ? BUILTIN_NOTE : USER_NOTE);

    overlay.hidden = current === null;
    if (current) {
      const check = checkPresetName(nameInput.value, nameContext(state, current));
      dialogTitle.textContent = current.mode === "rename" ? "Rename preset" : "Save preset";
      dialogNote.textContent = check.note;
      dialogNote.dataset.tone = check.tone;
      nameInput.classList.toggle("is-invalid", check.invalid);
      dialogPath.textContent = presetFilePath(check.trimmed);
      confirm.disabled = check.blocked;
      confirm.textContent = check.confirmText;
    }
  };

  session.store.subscribe(render);
  render(session.store.get());
}

function nameContext(state: SessionState, dialog: Dialog) {
  return {
    builtin: state.catalog.builtin,
    user: state.catalog.user.map((user) => user.name),
    mode: dialog.mode,
    from: dialog.from,
  };
}

const RENAME_ICON =
  '<svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true"><path d="M2 10l.5-2.5L8 2l2 2-5.5 5.5z"></path></svg>';
const DUPLICATE_ICON =
  '<svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true"><rect x="4" y="4" width="6.5" height="6.5" rx="1"></rect><path d="M8 4V2.5a1 1 0 0 0-1-1H2.5a1 1 0 0 0-1 1V7a1 1 0 0 0 1 1H4"></path></svg>';
const DELETE_ICON =
  '<svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true"><path d="M2 3h8M4.5 3V1.8h3V3M3 3l.6 7.2h4.8L9 3"></path></svg>';
