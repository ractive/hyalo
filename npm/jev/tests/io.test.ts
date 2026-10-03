import { test, expect } from "bun:test";
import { resolve } from "node:path";
import { childEnvironment, hyaloReader } from "../src/io.ts";

test("credential prefix is stripped from the child environment case-insensitively", () => {
  const env = childEnvironment({ TYPESAFE_API_KEY: "k", typesafe_api_key: "k", TypeSafe_Base_Url: "u", PATH: "/bin", TYPESAFEX: "keep" });
  expect(Object.keys(env).sort()).toEqual(["PATH", "TYPESAFEX"]);
});

test("rejects the npm .cmd/.bat shim; a native executable path is required", () => {
  for (const shim of ["hyalo.cmd", "hyalo.CMD", "hyalo.bat", "Hyalo.Bat"]) {
    expect(() => hyaloReader(resolve("/tools", shim))).toThrow("native hyalo executable");
  }
  expect(() => hyaloReader("relative/hyalo")).toThrow("absolute");
  expect(typeof hyaloReader(resolve("/tools/hyalo.exe"))).toBe("function");
});
