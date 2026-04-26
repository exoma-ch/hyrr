import { describe, it, expect } from "vitest";
import { PERIODIC_TABLE, ELEMENT_BY_SYMBOL, ELEMENT_BY_Z } from "./periodic-table-data";

describe("PERIODIC_TABLE — layout invariants", () => {
  it("contains exactly 118 elements", () => {
    expect(PERIODIC_TABLE).toHaveLength(118);
  });

  it("covers Z = 1..118 with no gaps and no duplicates", () => {
    const zs = PERIODIC_TABLE.map((c) => c.Z).sort((a, b) => a - b);
    expect(zs).toEqual(Array.from({ length: 118 }, (_, i) => i + 1));
  });

  it("places H at (1, 1) and He at (1, 18)", () => {
    const h = ELEMENT_BY_Z.get(1)!;
    const he = ELEMENT_BY_Z.get(2)!;
    expect([h.row, h.col]).toEqual([1, 1]);
    expect([he.row, he.col]).toEqual([1, 18]);
  });

  it("places La (Z=57) in main grid at row 6 col 3 (group 3)", () => {
    const la = ELEMENT_BY_SYMBOL.get("La")!;
    expect(la).toMatchObject({ row: 6, col: 3, group: 3, period: 6, block: "d" });
  });

  it("places Ac (Z=89) in main grid at row 7 col 3 (group 3)", () => {
    const ac = ELEMENT_BY_SYMBOL.get("Ac")!;
    expect(ac).toMatchObject({ row: 7, col: 3, group: 3, period: 7, block: "d" });
  });

  it("places lanthanides Ce..Lu on detached row 8 cols 4..17, all f-block", () => {
    for (let z = 58; z <= 71; z++) {
      const cell = ELEMENT_BY_Z.get(z)!;
      expect(cell.row).toBe(8);
      expect(cell.col).toBe(4 + (z - 58));
      expect(cell.block).toBe("f");
      expect(cell.period).toBe(6);
      expect(cell.group).toBeNull();
    }
  });

  it("places actinides Th..Lr on detached row 9 cols 4..17, all f-block", () => {
    for (let z = 90; z <= 103; z++) {
      const cell = ELEMENT_BY_Z.get(z)!;
      expect(cell.row).toBe(9);
      expect(cell.col).toBe(4 + (z - 90));
      expect(cell.block).toBe("f");
      expect(cell.period).toBe(7);
      expect(cell.group).toBeNull();
    }
  });

  it("places U (Z=92) in the actinide row at col 6", () => {
    expect(ELEMENT_BY_Z.get(92)).toMatchObject({ row: 9, col: 6, period: 7, block: "f" });
  });

  it("period 4 forms a full 18-column row from K to Kr", () => {
    for (let z = 19; z <= 36; z++) {
      const cell = ELEMENT_BY_Z.get(z)!;
      expect(cell.row).toBe(4);
      expect(cell.col).toBe(z - 18);
      expect(cell.period).toBe(4);
    }
  });

  it("period 5 forms a full 18-column row from Rb to Xe", () => {
    for (let z = 37; z <= 54; z++) {
      const cell = ELEMENT_BY_Z.get(z)!;
      expect(cell.row).toBe(5);
      expect(cell.col).toBe(z - 36);
      expect(cell.period).toBe(5);
    }
  });

  it("period 2 elements are split across the s/p block (cols 1-2 + 13-18)", () => {
    const cols: number[] = [];
    for (let z = 3; z <= 10; z++) cols.push(ELEMENT_BY_Z.get(z)!.col);
    expect(cols).toEqual([1, 2, 13, 14, 15, 16, 17, 18]);
  });

  it("blocks split: s (groups 1-2 + He), p (groups 13-18 except He), d (groups 3-12), f (lanthanides+actinides)", () => {
    // s-block sanity
    expect(ELEMENT_BY_SYMBOL.get("Li")!.block).toBe("s");
    expect(ELEMENT_BY_SYMBOL.get("Ca")!.block).toBe("s");
    expect(ELEMENT_BY_SYMBOL.get("He")!.block).toBe("s");
    // p-block
    expect(ELEMENT_BY_SYMBOL.get("B")!.block).toBe("p");
    expect(ELEMENT_BY_SYMBOL.get("Ne")!.block).toBe("p");
    expect(ELEMENT_BY_SYMBOL.get("Pb")!.block).toBe("p");
    // d-block
    expect(ELEMENT_BY_SYMBOL.get("Sc")!.block).toBe("d");
    expect(ELEMENT_BY_SYMBOL.get("Fe")!.block).toBe("d");
    expect(ELEMENT_BY_SYMBOL.get("Au")!.block).toBe("d");
    // La/Ac sit in the d-block in the wide form (group 3)
    expect(ELEMENT_BY_SYMBOL.get("La")!.block).toBe("d");
    expect(ELEMENT_BY_SYMBOL.get("Ac")!.block).toBe("d");
  });

  it("every cell with a non-null group has 1 ≤ group ≤ 18", () => {
    for (const cell of PERIODIC_TABLE) {
      if (cell.group !== null) {
        expect(cell.group).toBeGreaterThanOrEqual(1);
        expect(cell.group).toBeLessThanOrEqual(18);
      }
    }
  });

  it("symbol and Z are unique across the table", () => {
    const syms = new Set(PERIODIC_TABLE.map((c) => c.symbol));
    expect(syms.size).toBe(PERIODIC_TABLE.length);
    const zs = new Set(PERIODIC_TABLE.map((c) => c.Z));
    expect(zs.size).toBe(PERIODIC_TABLE.length);
  });
});
