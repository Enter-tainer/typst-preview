import { describe, expect, it } from "vitest";
import {
  PatchPair,
  interpretTargetView,
  changeViewPerspective,
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
  (MockElement | number | string)[][],
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
  const patches = patchPair.length
    ? [patchPair.map((i) => i.map(repr).join("->")).join(",")]
    : [];
  return [...instructions, ...patches];
}

const indexTargetView = (init: number, rearrange: (number | null)[]) =>
  interpretTargetView<MockElement>(
    injectOffsets("o", repeat(init)),
    injectOffsets("t", rearrange.map(reuseStub))
  );
const indexOriginView = (init: number, rearrange: (number | null)[]) =>
  changeViewPerspective<MockElement>(
    injectOffsets("o", repeat(init)),
    indexTargetView(init, rearrange)[0]
  );

describe("interpretView", () => {
  it("handleNoReuse", () => {
    const result = indexTargetView(1, [null]);
    expect(toSnapshot(result)).toMatchInlineSnapshot(`
      [
        "append,t0",
        "remove,0",
      ]
    `);
  });
  it("handleNoReuse_origin", () => {
    const result = indexOriginView(1, [null]);
    expect(toSnapshot([result, []])).toMatchInlineSnapshot(`
      [
        "remove,0",
        "insert,0,t0",
      ]
    `);
  });

  it("handleReuse", () => {
    const result = indexTargetView(1, [0]);
    expect(toSnapshot(result)).toMatchInlineSnapshot(`
      [
        "reuse,0",
      ]
    `);
  });
  it("handleReuse_origin", () => {
    const result = indexOriginView(1, [0]);
    expect(toSnapshot([result, []])).toMatchInlineSnapshot("[]");
  });

  it("handleMultipleReuse", () => {
    const result = indexTargetView(1, [0, 0]);
    expect(toSnapshot(result)).toMatchInlineSnapshot(`
      [
        "reuse,0",
        "append,t1",
      ]
    `);
  });
  it("handleMultipleReuse_origin", () => {
    const result = indexOriginView(1, [0, 0]);
    expect(toSnapshot([result, []])).toMatchInlineSnapshot(`
      [
        "insert,1,t1",
      ]
    `);
  });

  it("handleReuseRemove", () => {
    const result = indexTargetView(2, [1]);
    expect(toSnapshot(result)).toMatchInlineSnapshot(`
      [
        "reuse,1",
        "remove,0",
        "o1->t0",
      ]
    `);
  });
  it("handleReuseRemove_origin", () => {
    const result = indexOriginView(2, [1]);
    expect(toSnapshot([result, []])).toMatchInlineSnapshot(`
      [
        "remove,0",
      ]
    `);
  });

  it("handleReuseRemove2", () => {
    const result = indexTargetView(5, [2, 1, 4]);
    expect(toSnapshot(result)).toMatchInlineSnapshot(`
      [
        "reuse,2",
        "reuse,1",
        "reuse,4",
        "remove,0",
        "remove,3",
        "o2->t0,o4->t2",
      ]
    `);
  });
  it("handleReuseRemove2_origin", () => {
    const result = indexOriginView(5, [2, 1, 4]);
    expect(toSnapshot([result, []])).toMatchInlineSnapshot(`
      [
        "remove,0",
        "remove,2",
        "swap_in,0,1",
      ]
    `);
  });

  it("handleReuseInsert", () => {
    const result = indexTargetView(5, [null, 2, null, 1, null, 4, null]);
    expect(toSnapshot(result)).toMatchInlineSnapshot(`
      [
        "append,t0",
        "reuse,2",
        "append,t2",
        "reuse,1",
        "append,t4",
        "reuse,4",
        "append,t6",
        "remove,0",
        "remove,3",
        "o2->t1,o1->t3,o4->t5",
      ]
    `);
  });
  it("handleReuseInsert_origin", () => {
    const result = indexOriginView(5, [null, 2, null, 1, null, 4, null]);
    // after remove: [1, 2, 4]
    // swap_in,0,1: [2, 1, 4]
    // insert,0,t0: [t0, 2, 1, 4]
    // insert,2,t2: [t0, 2, t2, 1, 4]
    // insert,4,t4: [t0, 2, t2, 1, t4, 4]
    // insert,6,t6: [t0, 2, t2, 1, t4, 4, t6]

    expect(toSnapshot([result, []])).toMatchInlineSnapshot(`
      [
        "remove,0",
        "remove,2",
        "swap_in,0,1",
        "insert,0,t0",
        "insert,2,t2",
        "insert,4,t4",
        "insert,6,t6",
      ]
    `);
  });
});
