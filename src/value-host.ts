import type { ParamSpec } from "./engine.ts";
import type { Session } from "./session.ts";

export interface ValueHost {
  spec(id: string): ParamSpec;
  current(id: string): number;
  record(): void;
  setValue(id: string, value: number, record?: boolean): void;
  setValues(values: Record<string, number>, record?: boolean): void;
  restoreValue(id: string, value: number): void;
}

export function hostFromSession(session: Session): ValueHost {
  return {
    spec: (id) => session.spec(id),
    current: (id) => session.current(id),
    record: () => session.record(),
    setValue: (id, value, record) => session.setValue(id, value, record),
    setValues: (values, record) => session.setValues(values, record),
    restoreValue: (id, value) => session.restoreValue(id, value),
  };
}
