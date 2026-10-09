import assert from "node:assert/strict";
import { test } from "node:test";
import { formatCount, formatDb, formatHz, formatMs } from "../src/format.ts";

test("formatters round and group the way the readouts expect", () => {
  assert.equal(formatDb(-18.04), "-18.0 dB");
  assert.equal(formatMs(31.4), "31 ms");
  assert.equal(formatHz(48000), "48 000 Hz");
  assert.equal(formatCount(1234567), "1 234 567");
  assert.equal(formatCount(999), "999");
});
