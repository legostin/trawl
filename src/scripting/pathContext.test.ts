import { describe, expect, it } from "vitest";
import { extractPathLiterals, pathArgContext } from "./pathContext";

describe("pathArgContext", () => {
  it("курсор внутри строки-пути → fn и prefix", () => {
    const line = "patch(res, 'items[*].adv";
    expect(pathArgContext(line, line.length + 1)).toEqual({ fn: "patch", prefix: "items[*].adv" });
  });
  it("двойные кавычки и пустой префикс", () => {
    const line = 'pick(res, "';
    expect(pathArgContext(line, line.length + 1)).toEqual({ fn: "pick", prefix: "" });
  });
  it("вне строки-пути → null", () => {
    expect(pathArgContext("patch(res, ", 12)).toBeNull();
    expect(pathArgContext("someFn(res, 'a.b", 17)).toBeNull();
    expect(pathArgContext("patch(res, 'a.b', ", 19)).toBeNull(); // строка закрыта
  });
});

describe("extractPathLiterals", () => {
  it("находит все литералы с координатами", () => {
    const script = "const r = send(request);\npatch(r, 'items[*].x', 1);\npick(r, \"a\");";
    const lits = extractPathLiterals(script);
    expect(lits).toHaveLength(2);
    expect(lits[0]).toMatchObject({ path: "items[*].x", line: 2 });
    // столбцы: путь начинается сразу после кавычки
    expect(script.split("\n")[1].slice(lits[0].startColumn - 1, lits[0].endColumn - 1)).toBe("items[*].x");
    expect(lits[1]).toMatchObject({ path: "a", line: 3 });
  });
  it("динамический путь (переменная) пропускается", () => {
    expect(extractPathLiterals("patch(r, dyn, 1)")).toHaveLength(0);
  });
});
