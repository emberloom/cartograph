import { describe } from "vitest";
import * as fs from "node:fs";
import { spawn } from "node:child_process";

export function run(): void {
    describe("test", () => {});
    fs.readFileSync("x");
    spawn("ls", []);
}
