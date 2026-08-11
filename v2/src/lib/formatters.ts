/** 格式化 JSON 字符串（2 空格缩进）；非法 JSON 抛错。 */
export function formatJSON(value: string): string {
  const trimmed = value.trim();
  if (!trimmed) return "";
  const parsed = JSON.parse(trimmed);
  return JSON.stringify(parsed, null, 2);
}
