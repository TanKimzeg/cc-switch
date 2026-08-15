import { describe, expect, it } from "vitest";
import {
  filterInstalledSkills,
  isSkillInstalled,
  parseRepoUrl,
  skillErrorText,
} from "./skillsUtils";

function t(key: string, vars?: Record<string, string>) {
  if (key === "skillsError.http404") return "仓库或分支不存在，请检查 URL";
  if (key === "skillsError.directoryConflict") {
    return `技能目录 '${vars?.directory}' 已被 ${vars?.existingRepo} 占用，无法从 ${vars?.newRepo} 安装`;
  }
  if (key === "skillsError.checkNetwork") return "请检查网络连接";
  if (key === "skillsError.downloadFailed") {
    return `下载失败（HTTP ${vars?.status}）`;
  }
  if (key === "skillsError.uninstallFirst") return "请先卸载已安装的同名技能";
  return key;
}

describe("parseRepoUrl", () => {
  it("accepts owner/name", () => {
    expect(parseRepoUrl("anthropics/skills")).toEqual({
      owner: "anthropics",
      name: "skills",
    });
  });

  it("accepts https URLs with optional .git and trailing path", () => {
    expect(parseRepoUrl("https://github.com/anthropics/skills")).toEqual({
      owner: "anthropics",
      name: "skills",
    });
    expect(parseRepoUrl("https://github.com/o/r.git")).toEqual({
      owner: "o",
      name: "r",
    });
    expect(parseRepoUrl("https://github.com/o/r/tree/main/skills")).toEqual({
      owner: "o",
      name: "r",
    });
  });

  it("rejects invalid inputs", () => {
    expect(parseRepoUrl("")).toBeNull();
    expect(parseRepoUrl("owner/name/extra")).toBeNull();
    expect(parseRepoUrl("just-name")).toBeNull();
    expect(parseRepoUrl("own%er/repo")).toBeNull();
    expect(parseRepoUrl("https://evil.com/o/r")).toBeNull();
  });
});

describe("isSkillInstalled", () => {
  const installed = [
    { directory: "find-skills", repoOwner: "anthropics", repoName: "skills" },
  ];

  it("matches case-insensitively", () => {
    expect(
      isSkillInstalled(installed, "FIND-Skills", "ANTHROPICS", "Skills"),
    ).toBe(true);
    expect(isSkillInstalled(installed, "other", "anthropics", "skills")).toBe(
      false,
    );
  });
});

describe("filterInstalledSkills", () => {
  const skills = [
    { id: "a", name: "Web Dev", description: "build sites", directory: "web" },
    {
      id: "b",
      name: "Docs",
      description: null,
      directory: "docs",
      repoOwner: "o",
      repoName: "r",
    },
  ];

  it("filters by name/description/repo", () => {
    expect(filterInstalledSkills(skills, "")).toHaveLength(2);
    expect(filterInstalledSkills(skills, "sites")).toHaveLength(1);
    expect(filterInstalledSkills(skills, "o/r")).toHaveLength(1);
    expect(filterInstalledSkills(skills, "zzz")).toHaveLength(0);
  });
});

describe("skillErrorText", () => {
  it("formats structured JSON errors and appends suggestion hint", () => {
    const err = JSON.stringify({
      code: "DOWNLOAD_FAILED",
      context: { status: "404" },
      suggestion: "http404",
    });
    expect(skillErrorText(t, err)).toContain("下载失败");
    expect(skillErrorText(t, err)).toContain("仓库或分支不存在");
  });

  it("formats via error code with context and suggestion", () => {
    const err = JSON.stringify({
      code: "SKILL_DIRECTORY_CONFLICT",
      context: { directory: "web", existingRepo: "a/b", newRepo: "c/d" },
      suggestion: "uninstallFirst",
    });
    expect(skillErrorText(t, err)).toContain("web");
    expect(skillErrorText(t, err)).toContain("a/b");
    expect(skillErrorText(t, err)).toContain("请先卸载");
  });

  it("falls back to raw string", () => {
    expect(skillErrorText(t, "plain error")).toBe("plain error");
    expect(skillErrorText(t, "{not json")).toBe("{not json");
    expect(skillErrorText(t, 42)).toBe("42");
  });
});
