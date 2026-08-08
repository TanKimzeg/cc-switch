// ProviderForm 配置编辑器渲染（per-app 渲染函数）。
// ProviderFormFull 通过描述符的 `renderConfigEditor(ctx)` 派发，本文件按 app 提供实现，
// 注册表（providerFormRegistry.ts）只登记引用——新增 app 时在此追加渲染函数并在注册表登记。

import type { ReactNode } from "react";
import { Label } from "@/components/ui/label";
import JsonEditor from "@/components/JsonEditor";
import CodexConfigEditor from "./CodexConfigEditor";
import GeminiConfigEditor from "./GeminiConfigEditor";
import { CommonConfigEditor } from "./CommonConfigEditor";
import { ClaudeFormFields } from "./ClaudeFormFields";
import { CodexFormFields } from "./CodexFormFields";
import { GeminiFormFields } from "./GeminiFormFields";
import { OpenCodeFormFields } from "./OpenCodeFormFields";
import { OpenClawFormFields } from "./OpenClawFormFields";
import { HermesFormFields } from "./HermesFormFields";
import { OmoFormFields } from "./OmoFormFields";
import type { ProviderFormRenderContext } from "./ProviderFormRenderContext";

const OPENCODE_CONFIG_EDITOR_PLACEHOLDER = `{
  "npm": "@ai-sdk/openai-compatible",
  "options": {
    "baseURL": "https://your-api-endpoint.com",
    "apiKey": "your-api-key-here"
  },
  "models": {}
}`;

const OPENCLAW_CONFIG_EDITOR_PLACEHOLDER = `{
  "baseUrl": "https://api.example.com/v1",
  "apiKey": "your-api-key-here",
  "api": "openai-completions",
  "models": []
}`;

const HERMES_CONFIG_EDITOR_PLACEHOLDER = `{
  "name": "my-provider",
  "base_url": "https://api.example.com/v1",
  "api_key": ""
}`;

/** Claude 专属字段 */
export function renderClaudeFormFields(
  ctx: ProviderFormRenderContext,
): ReactNode {
  return <ClaudeFormFields {...ctx.claude} />;
}

/** Codex 专属字段 */
export function renderCodexFormFields(
  ctx: ProviderFormRenderContext,
): ReactNode {
  return <CodexFormFields {...ctx.codex} />;
}

/** Gemini 专属字段 */
export function renderGeminiFormFields(
  ctx: ProviderFormRenderContext,
): ReactNode {
  return <GeminiFormFields {...ctx.gemini} />;
}

/** OpenCode 专属字段：OMO 类目渲染 OmoFormFields，否则渲染 OpenCodeFormFields */
export function renderOpenCodeFormFields(
  ctx: ProviderFormRenderContext,
): ReactNode {
  const { category } = ctx;
  if (category === "omo" || category === "omo-slim") {
    return <OmoFormFields {...ctx.omo} />;
  }
  return <OpenCodeFormFields {...ctx.opencode} />;
}

/** OpenClaw 专属字段 */
export function renderOpenclawFormFields(
  ctx: ProviderFormRenderContext,
): ReactNode {
  return <OpenClawFormFields {...ctx.openclaw} />;
}

/** Hermes 专属字段 */
export function renderHermesFormFields(
  ctx: ProviderFormRenderContext,
): ReactNode {
  return <HermesFormFields {...ctx.hermes} />;
}

/** Codex 配置编辑器（带 settingsConfig 错误插槽） */
export function renderCodexConfigEditor(
  ctx: ProviderFormRenderContext,
): ReactNode {
  const { form, category, isProxyTakeover, settingsConfigErrorField } = ctx;
  const c = ctx.configEditor.codex;
  return (
    <>
      <CodexConfigEditor
        authValue={c.authValue}
        configValue={c.configValue}
        providerName={form.watch("name")}
        showRemoteCompaction={category !== "official"}
        isProxyTakeover={isProxyTakeover}
        onAuthChange={c.onAuthChange}
        onConfigChange={c.onConfigChange}
        useCommonConfig={c.useCommonConfig}
        onCommonConfigToggle={c.onCommonConfigToggle}
        commonConfigSnippet={c.commonConfigSnippet}
        onCommonConfigSnippetChange={c.onCommonConfigSnippetChange}
        onCommonConfigErrorClear={c.onCommonConfigErrorClear}
        commonConfigError={c.commonConfigError}
        authError={c.authError}
        configError={c.configError}
        onExtract={c.onExtract}
        isExtracting={c.isExtracting}
      />
      {settingsConfigErrorField}
    </>
  );
}

/** Gemini 配置编辑器（带 settingsConfig 错误插槽） */
export function renderGeminiConfigEditor(
  ctx: ProviderFormRenderContext,
): ReactNode {
  const { settingsConfigErrorField } = ctx;
  const g = ctx.configEditor.gemini;
  return (
    <>
      <GeminiConfigEditor
        envValue={g.envValue}
        configValue={g.configValue}
        onEnvChange={g.onEnvChange}
        onConfigChange={g.onConfigChange}
        useCommonConfig={g.useCommonConfig}
        onCommonConfigToggle={g.onCommonConfigToggle}
        commonConfigSnippet={g.commonConfigSnippet}
        onCommonConfigSnippetChange={g.onCommonConfigSnippetChange}
        onCommonConfigErrorClear={g.onCommonConfigErrorClear}
        commonConfigError={g.commonConfigError}
        envError={g.envError}
        configError={g.configError}
        onExtract={g.onExtract}
        isExtracting={g.isExtracting}
      />
      {settingsConfigErrorField}
    </>
  );
}

/** OMO（OpenCode）只读 JSON 预览 */
export function renderOpenCodeOmoConfigEditor(
  ctx: ProviderFormRenderContext,
): ReactNode {
  const { t, isDarkMode, omoJsonPreview } = ctx;
  return (
    <div className="space-y-2">
      <Label>{t("provider.configJson")}</Label>
      <JsonEditor
        value={omoJsonPreview}
        onChange={() => {}}
        rows={14}
        showValidation={false}
        language="json"
        darkMode={isDarkMode}
      />
    </div>
  );
}

/** 通用原始 settingsConfig JSON 编辑器 + 错误插槽 */
function renderRawJsonConfigEditor(
  ctx: ProviderFormRenderContext,
  placeholder?: string,
): ReactNode {
  const { t, form, isDarkMode, settingsConfigErrorField } = ctx;
  return (
    <>
      <div className="space-y-2">
        <Label htmlFor="settingsConfig">{t("provider.configJson")}</Label>
        <JsonEditor
          value={form.getValues("settingsConfig")}
          onChange={(config) => form.setValue("settingsConfig", config)}
          placeholder={placeholder}
          rows={14}
          showValidation={true}
          language="json"
          darkMode={isDarkMode}
        />
      </div>
      {settingsConfigErrorField}
    </>
  );
}

/** OpenCode 非 OMO 的 settingsConfig JSON 编辑器 */
export function renderOpenCodeJsonConfigEditor(
  ctx: ProviderFormRenderContext,
): ReactNode {
  return renderRawJsonConfigEditor(ctx, OPENCODE_CONFIG_EDITOR_PLACEHOLDER);
}

/** OpenClaw settingsConfig JSON 编辑器 */
export function renderOpenclawConfigEditor(
  ctx: ProviderFormRenderContext,
): ReactNode {
  return renderRawJsonConfigEditor(ctx, OPENCLAW_CONFIG_EDITOR_PLACEHOLDER);
}

/** Hermes settingsConfig JSON 编辑器 */
export function renderHermesConfigEditor(
  ctx: ProviderFormRenderContext,
): ReactNode {
  return renderRawJsonConfigEditor(ctx, HERMES_CONFIG_EDITOR_PLACEHOLDER);
}

/** OpenCode 配置编辑器：按 category 派发 OMO / 非 OMO 分支 */
export function renderOpenCodeConfigEditor(
  ctx: ProviderFormRenderContext,
): ReactNode {
  const { category } = ctx;
  if (category === "omo" || category === "omo-slim") {
    return renderOpenCodeOmoConfigEditor(ctx);
  }
  return renderOpenCodeJsonConfigEditor(ctx);
}

/** Claude 通用配置编辑器（CommonConfigEditor + 错误插槽） */
export function renderClaudeConfigEditor(
  ctx: ProviderFormRenderContext,
): ReactNode {
  const { form, settingsConfigErrorField } = ctx;
  const c = ctx.configEditor.claude;
  return (
    <>
      <CommonConfigEditor
        value={form.getValues("settingsConfig")}
        onChange={(value) => form.setValue("settingsConfig", value)}
        useCommonConfig={c.useCommonConfig}
        onCommonConfigToggle={c.onCommonConfigToggle}
        commonConfigSnippet={c.commonConfigSnippet}
        onCommonConfigSnippetChange={c.onCommonConfigSnippetChange}
        commonConfigError={c.commonConfigError}
        onEditClick={c.onEditClick}
        isModalOpen={c.isModalOpen}
        onModalClose={c.onModalClose}
        onExtract={c.onExtract}
        isExtracting={c.isExtracting}
      />
      {settingsConfigErrorField}
    </>
  );
}
