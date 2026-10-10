export type Listener<S> = (state: S, previous: S) => void;

export interface Store<S> {
  get(): S;
  set(update: (state: S) => S): void;
  patch(changes: Partial<S>): void;
  subscribe(listener: Listener<S>): () => void;
}

export function createStore<S extends object>(initial: S): Store<S> {
  let state = initial;
  const listeners = new Set<Listener<S>>();
  const notify = (previous: S): void => {
    for (const listener of [...listeners]) {
      listener(state, previous);
    }
  };
  return {
    get: () => state,
    set(update) {
      const previous = state;
      const next = update(state);
      if (next === previous) {
        return;
      }
      state = next;
      notify(previous);
    },
    patch(changes) {
      const previous = state;
      state = { ...state, ...changes };
      notify(previous);
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}
