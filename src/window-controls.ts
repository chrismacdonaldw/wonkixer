export const RESIZE_DIRECTIONS = [
  "North",
  "NorthEast",
  "East",
  "SouthEast",
  "South",
  "SouthWest",
  "West",
  "NorthWest",
] as const;

export type ResizeDirection = (typeof RESIZE_DIRECTIONS)[number];

export function isResizeDirection(value: unknown): value is ResizeDirection {
  return typeof value === "string" && (RESIZE_DIRECTIONS as readonly string[]).includes(value);
}

export interface WindowControls {
  minimize(): Promise<void>;
  toggleMaximize(): Promise<void>;
  close(): Promise<void>;
  isMaximized(): Promise<boolean>;
  startDragging(): Promise<void>;
  startResizeDragging(direction: ResizeDirection): Promise<void>;
  onResized(handler: () => void): Promise<() => void>;
}
