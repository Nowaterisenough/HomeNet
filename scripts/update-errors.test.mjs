import assert from "node:assert/strict";
import test from "node:test";
import { operationError } from "../src/utils/errors.ts";
test("update error messages contain one operation prefix", () => {
  for (const error of [
    "GitHub HTTP 403",
    "检查更新失败：GitHub HTTP 403",
    "检查更新失败: 检查更新失败：GitHub HTTP 403",
    new Error("检查或安装更新失败：GitHub HTTP 403"),
  ]) {
    assert.equal(
      operationError("检查更新失败", error),
      "检查更新失败：GitHub HTTP 403",
    );
  }
});
