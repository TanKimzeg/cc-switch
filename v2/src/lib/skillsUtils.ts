/** 后端 `format_skill_error` 生成的 JSON 错误负载。 */
interface SkillErrorPayload {
  code?: string;
  context?: Record<string, string>;
  suggestion?: string | null;
}

/** 错误码 → i18n 键（skillsError.*）。 */
const CODE_KEYS: Record<string, string> = {
  SKILL_DIRECTORY_CONFLICT: "directoryConflict",
  DOWNLOAD_TIMEOUT: "downloadTimeout",
  SKILL_DIR_NOT_FOUND: "skillDirNotFound",
  INVALID_SKILL_DIRECTORY: "invalidSkillDirectory",
  DOWNLOAD_FAILED: "downloadFailed",
};

/** 结构化建议 → i18n 键（skillsError.*，优先于错误码）。 */
const SUGGESTION_KEYS = [
  "http403",
  "http404",
  "http429",
  "checkNetwork",
  "checkZipContent",
  "checkRepoUrl",
  "uninstallFirst",
];

/** 翻译函数的最小签名（便于测试传入简单 mock）。 */
export type SkillsT = (key: string, vars?: Record<string, string>) => string;

/** 把后端 JSON 错误字符串格式化为可读文案。 */
export function skillErrorText(t: SkillsT, err: unknown): string {
  if (typeof err !== "string") return String(err);
  let payload: SkillErrorPayload | null = null;
  try {
    const parsed = JSON.parse(err) as unknown;
    if (parsed && typeof parsed === "object" && "code" in (parsed as object)) {
      payload = parsed as SkillErrorPayload;
    }
  } catch {
    return err;
  }
  if (!payload) return err;
  const ctx = payload.context ?? {};
  const codeKey = payload.code ? CODE_KEYS[payload.code] : undefined;
  let main = codeKey ? t(`skillsError.${codeKey}`, ctx) : err;
  if (payload.suggestion && SUGGESTION_KEYS.includes(payload.suggestion)) {
    const hint = t(`skillsError.${payload.suggestion}`, ctx);
    if (hint !== `skillsError.${payload.suggestion}` && !main.includes(hint)) {
      main = `${main}\n${hint}`;
    }
  }
  return main;
}

/**
 * 解析仓库 URL 输入。
 *
 * 支持 `owner/name`、`https://github.com/owner/name`（含可选 `.git` 后缀）。
 * 不合法返回 null。
 */
export function parseRepoUrl(
  input: string,
): { owner: string; name: string } | null {
  const trimmed = input.trim();
  if (!trimmed) return null;

  let repoPart = trimmed;
  const httpsMatch =
    /^https?:\/\/github\.com\/([^/]+\/[^/]+?)(?:\.git)?(\/.*)?$/i.exec(trimmed);
  if (httpsMatch) {
    repoPart = httpsMatch[1];
  } else {
    repoPart = repoPart.replace(/\.git$/, "");
  }
  const parts = repoPart.split("/").filter(Boolean);
  if (parts.length !== 2) return null;
  if (!/^[A-Za-z0-9-]+$/.test(parts[0])) return null;
  if (!/^[A-Za-z0-9._-]+$/.test(parts[1])) return null;
  return { owner: parts[0], name: parts[1] };
}

/** 判断某技能（按目录 + 仓库）是否已安装。 */
export function isSkillInstalled(
  installed: ReadonlyArray<{
    directory: string;
    repoOwner?: string | null;
    repoName?: string | null;
  }>,
  directory: string,
  repoOwner: string,
  repoName: string,
): boolean {
  return installed.some(
    (s) =>
      s.directory.toLowerCase() === directory.toLowerCase() &&
      s.repoOwner?.toLowerCase() === repoOwner.toLowerCase() &&
      s.repoName?.toLowerCase() === repoName.toLowerCase(),
  );
}

/** 过滤已安装技能（名称/描述/目录/仓库）。 */
export function filterInstalledSkills<
  T extends {
    name: string;
    id: string;
    description?: string | null;
    directory: string;
    repoOwner?: string | null;
    repoName?: string | null;
  },
>(skills: readonly T[], query: string): T[] {
  const q = query.trim().toLowerCase();
  if (!q) return [...skills];
  return skills.filter((s) => {
    const repo = [s.repoOwner, s.repoName].filter(Boolean).join("/");
    return [s.name, s.id, s.description, s.directory, repo]
      .join(" ")
      .toLowerCase()
      .includes(q);
  });
}
