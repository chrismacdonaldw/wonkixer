export const HISTORY_LIMIT = 100;

export interface History<T> {
  push(snapshot: T): void;
  dropLast(): void;
  undo(current: T): T | null;
  redo(current: T): T | null;
  undoDepth(): number;
  redoDepth(): number;
}

export function createHistory<T>(limit: number = HISTORY_LIMIT): History<T> {
  let past: T[] = [];
  let future: T[] = [];
  return {
    push(snapshot) {
      past = [...past, snapshot].slice(-limit);
      future = [];
    },
    dropLast() {
      past = past.slice(0, -1);
    },
    undo(current) {
      const previous = past[past.length - 1];
      if (previous === undefined) {
        return null;
      }
      past = past.slice(0, -1);
      future = [...future, current];
      return previous;
    },
    redo(current) {
      const next = future[future.length - 1];
      if (next === undefined) {
        return null;
      }
      future = future.slice(0, -1);
      past = [...past, current];
      return next;
    },
    undoDepth: () => past.length,
    redoDepth: () => future.length,
  };
}
