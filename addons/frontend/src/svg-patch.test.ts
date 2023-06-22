import { describe, expect, it } from "vitest";
import {
  PatchPair,
  TargetViewInstruction,
  interpretTargetView,
} from "./svg-patch";

interface Attributes {
  [key: string]: string | null | undefined;
  "data-tid"?: string | null;
  "data-kind"?: string | null;
  "data-reuse-from"?: string | null;
}

class MockElement {
  tagName = "g";

  constructor(public attrs: Attributes) {}

  getAttribute(s: string): string | null {
    return this.attrs[s] ?? null;
  }
}

const injectOffsets = (kind: string, elems: MockElement[]): MockElement[] => {
  for (let i = 0; i < elems.length; i++) {
    elems[i].attrs["data-kind"] = kind;
    elems[i].attrs["data-tid"] = i.toString();
  }

  return elems;
};

const repeat = (n: number): MockElement[] => {
  const res: MockElement[] = [];
  for (let i = 0; i < n; i++) {
    res.push(new MockElement({}));
  }

  return res;
};

const reuseStub = (n: number | null) =>
  new MockElement({
    "data-reuse-from": n !== null ? n.toString() : null,
  });

function toSnapshot([targetView, patchPair]: [
  TargetViewInstruction<MockElement>[],
  PatchPair<MockElement>[]
]): string[] {
  const repr = (elem: unknown) => {
    if (elem instanceof MockElement) {
      return (elem.attrs["data-kind"] || "") + elem.attrs["data-tid"];
    }
    return elem;
  };

  const instructions = targetView.map((i) => {
    return i.map(repr).join(",");
  });
  const patches = patchPair.map((i) => i.map(repr));
  return [...instructions, patches.join("+")];
}

describe("interpretTargetView", () => {
  const testReuse = (init: number, rearrange: (number | null)[]) =>
    interpretTargetView<MockElement>(
      injectOffsets("o", repeat(init)),
      injectOffsets("t", rearrange.map(reuseStub))
    );

  it("handleNoReuse", () => {
    const result = testReuse(1, [null]);
    expect(toSnapshot(result)).toMatchInlineSnapshot(`
      [
        "append,t0",
        "remove,0",
        "",
      ]
    `);
  });

  it("handleReuse", () => {
    const result = testReuse(1, [0]);
    expect(toSnapshot(result)).toMatchInlineSnapshot(`
      [
        "reuse,0",
        "",
      ]
    `);
  });

  it("handleMultipleReuse", () => {
    const result = testReuse(1, [0, 0]);
    expect(toSnapshot(result)).toMatchInlineSnapshot(`
      [
        "reuse,0",
        "append,t1",
        "",
      ]
    `);
  });

  it("handleReuseRemove", () => {
    const result = testReuse(2, [1]);
    expect(toSnapshot(result)).toMatchInlineSnapshot(`
      [
        "reuse,1",
        "remove,0",
        "o1,t0",
      ]
    `);
  });
});
