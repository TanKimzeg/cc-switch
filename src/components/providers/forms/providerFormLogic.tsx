// ProviderForm 逻辑函数（per-app 实现）：applyPreset / resetCustom。
// 每个 app 在注册表中登记自己的实现，ProviderFormFull 只调用 descriptor 槽位，
// 新增 app 无需改动核心的 handlePresetChange。

import type { ProviderPreset } from "@/config/claudeProviderPresets";
import type { CodexProviderPreset } from "@/config/codexProviderPresets";
import type { GeminiProviderPreset } from "@/config/geminiProviderPresets";
import type { OpenCodeProviderPreset } from "@/config/opencodeProviderPresets";
import type { OpenClawProviderPreset } from "@/config/openclawProviderPresets";
import type { HermesProviderPreset } from "@/config/hermesProviderPresets";
import { getCodexCustomTemplate } from "@/config/codexTemplates";
import {
  applyTemplateValues,
  codexApiFormatFromWireApi,
  extractCodexWireApi,
} from "@/utils/providerConfigUtils";
import type { ProviderFormLogicContext } from "./ProviderFormLogicContext";

export function resetCodexCustomState(ctx: ProviderFormLogicContext): void {
  const template = getCodexCustomTemplate();
  ctx.codex.resetCodexConfig(template.auth, template.config);
  ctx.codex.setCodexChatReasoning({});
  ctx.codex.setPromptCacheRouting("auto");
  ctx.codex.setLocalCodexApiFormat(
    codexApiFormatFromWireApi(extractCodexWireApi(template.config)) ??
      "openai_responses",
  );
}

export function resetGeminiCustomState(ctx: ProviderFormLogicContext): void {
  ctx.gemini.resetGeminiConfig({}, {});
}

export function resetOpenCodeCustomState(ctx: ProviderFormLogicContext): void {
  ctx.opencode.resetOpencodeState();
  ctx.opencode.resetOmoDraftState();
}

export function resetOpenclawCustomState(ctx: ProviderFormLogicContext): void {
  ctx.openclaw.resetOpenclawState();
}

export function resetHermesCustomState(ctx: ProviderFormLogicContext): void {
  ctx.hermes.resetHermesState();
}

export function applyClaudePreset(
  preset: ProviderPreset,
  ctx: ProviderFormLogicContext,
): void {
  const { t, form } = ctx;
  const config = applyTemplateValues(
    preset.settingsConfig,
    preset.templateValues,
  );

  if (preset.apiFormat) {
    ctx.claude.setLocalApiFormat(preset.apiFormat);
  } else {
    ctx.claude.setLocalApiFormat("anthropic");
  }

  ctx.claude.setLocalApiKeyField(preset.apiKeyField ?? "ANTHROPIC_AUTH_TOKEN");
  ctx.claude.setLocalIsFullUrl(false);

  form.reset({
    name: preset.nameKey ? t(preset.nameKey) : preset.name,
    websiteUrl: preset.websiteUrl ?? "",
    settingsConfig: JSON.stringify(config, null, 2),
    icon: preset.icon ?? "",
    iconColor: preset.iconColor ?? "",
  });
}

export function applyCodexPreset(
  preset: CodexProviderPreset,
  ctx: ProviderFormLogicContext,
): void {
  const { t, form } = ctx;
  const auth = preset.auth ?? {};
  const config = preset.config ?? "";

  ctx.codex.resetCodexConfig(auth, config, preset.modelCatalog ?? []);
  ctx.codex.setCodexChatReasoning(preset.codexChatReasoning ?? {});
  ctx.codex.setPromptCacheRouting(preset.promptCacheRouting ?? "auto");
  ctx.codex.setLocalCodexApiFormat(
    preset.apiFormat ??
      codexApiFormatFromWireApi(extractCodexWireApi(config)) ??
      "openai_responses",
  );

  form.reset({
    name: preset.nameKey ? t(preset.nameKey) : preset.name,
    websiteUrl: preset.websiteUrl ?? "",
    settingsConfig: JSON.stringify({ auth, config }, null, 2),
    icon: preset.icon ?? "",
    iconColor: preset.iconColor ?? "",
  });
}

export function applyGeminiPreset(
  preset: GeminiProviderPreset,
  ctx: ProviderFormLogicContext,
): void {
  const { t, form } = ctx;
  const env = (preset.settingsConfig as any)?.env ?? {};
  const config = (preset.settingsConfig as any)?.config ?? {};

  ctx.gemini.resetGeminiConfig(env, config);

  form.reset({
    name: preset.nameKey ? t(preset.nameKey) : preset.name,
    websiteUrl: preset.websiteUrl ?? "",
    settingsConfig: JSON.stringify(preset.settingsConfig, null, 2),
    icon: preset.icon ?? "",
    iconColor: preset.iconColor ?? "",
  });
}

export function applyOpenCodePreset(
  preset: OpenCodeProviderPreset,
  ctx: ProviderFormLogicContext,
): void {
  const { t, form } = ctx;
  const config = preset.settingsConfig;

  if (preset.category === "omo" || preset.category === "omo-slim") {
    ctx.opencode.resetOmoDraftState();
    form.reset({
      name: preset.category === "omo" ? "OMO" : "OMO Slim",
      websiteUrl: preset.websiteUrl ?? "",
      settingsConfig: JSON.stringify({}, null, 2),
      icon: preset.icon ?? "",
      iconColor: preset.iconColor ?? "",
    });
    return;
  }

  ctx.opencode.resetOpencodeState(config);

  form.reset({
    name: preset.nameKey ? t(preset.nameKey) : preset.name,
    websiteUrl: preset.websiteUrl ?? "",
    settingsConfig: JSON.stringify(config, null, 2),
    icon: preset.icon ?? "",
    iconColor: preset.iconColor ?? "",
  });
}

export function applyOpenclawPreset(
  id: string,
  preset: OpenClawProviderPreset,
  ctx: ProviderFormLogicContext,
): void {
  const { t, form } = ctx;
  const config = preset.settingsConfig;

  // Update activePreset with suggestedDefaults for OpenClaw
  ctx.setActivePreset({
    id,
    category: preset.category,
    isPartner: preset.isPartner,
    partnerPromotionKey: preset.partnerPromotionKey,
    suggestedDefaults: preset.suggestedDefaults,
  });

  ctx.openclaw.resetOpenclawState(config);

  form.reset({
    name: preset.nameKey ? t(preset.nameKey) : preset.name,
    websiteUrl: preset.websiteUrl ?? "",
    settingsConfig: JSON.stringify(config, null, 2),
    icon: preset.icon ?? "",
    iconColor: preset.iconColor ?? "",
  });
}

export function applyHermesPreset(
  _id: string,
  preset: HermesProviderPreset,
  ctx: ProviderFormLogicContext,
): void {
  const { t, form } = ctx;
  const config = preset.settingsConfig;

  ctx.hermes.resetHermesState(config);

  form.reset({
    name: preset.nameKey ? t(preset.nameKey) : preset.name,
    websiteUrl: preset.websiteUrl ?? "",
    settingsConfig: JSON.stringify(config, null, 2),
    icon: preset.icon ?? "",
    iconColor: preset.iconColor ?? "",
  });
}
