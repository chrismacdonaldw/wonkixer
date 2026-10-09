const THOUSANDS_SEPARATOR = " ";

export function formatDb(value: number): string {
  return `${value.toFixed(1)} dB`;
}

export function formatMs(value: number): string {
  return `${Math.round(value)} ms`;
}

export function formatHz(value: number): string {
  return `${formatCount(value)} Hz`;
}

export function formatCount(value: number): string {
  return Math.round(value)
    .toString()
    .replace(/\B(?=(\d{3})+(?!\d))/g, THOUSANDS_SEPARATOR);
}
