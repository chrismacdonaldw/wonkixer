export function describeError(error: unknown): string {
  return String(error).replace(/^Error:\s*/i, "");
}
