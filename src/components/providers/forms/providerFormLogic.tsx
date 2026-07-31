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
  extractCodexModelName,
  extractCodexWireApi,
  setCodexModelName as setCodexModelNameInConfig,
  setCodexWireApi,
} from "@/utils/providerConfigUtils";
import { parseOmoOtherFieldsObject } from "@/types/omo";
import type { CodexCatalogModel } from "@/types";
import type { ProviderFormData } from "@/lib/schemas/provider";
import type { ProviderFormLogicContext } from "./ProviderFormLogicContext";

export const normalizeCodexCatalogModelsForSave = (
  models: CodexCatalogModel[],
): CodexCatalogModel[] => {
  const seen = new Set<string>();
  const normalized: CodexCatalogModel[] = [];

  for (const item of models) {
    const model = item.model.trim();
    if (!model || seen.has(model)) continue;
    seen.add(model);

    const displayName = item.displayName?.trim();
    const rawContextWindow = String(item.contextWindow ?? "").replace(
      /[^\d]/g,
      "",
    );
    const contextWindow = rawContextWindow
      ? Number.parseInt(rawContextWindow, 10)
      : undefined;

    const inputModalities = item.inputModalities?.filter(
      (m) => typeof m === "string" && m.trim(),
    );

    const baseInstructions = item.baseInstructions?.trim();

    normalized.push({
      model,
      ...(displayName ? { displayName } : {}),
      ...(contextWindow && contextWindow > 0 ? { contextWindow } : {}),
      // Native Responses profile overrides (ignored by the chat/proxy profile).
      ...(typeof item.supportsParallelToolCalls === "boolean"
        ? { supportsParallelToolCalls: item.supportsParallelToolCalls }
        : {}),
      ...(inputModalities && inputModalities.length > 0
        ? { inputModalities }
        : {}),
      ...(baseInstructions ? { baseInstructions } : {}),
    });
  }

  return normalized;
};

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
    settingsConfig: JSON.stringify(config, null, 2),
    icon: preset.icon ?? "",
    iconColor: preset.iconColor ?? "",
  });
}

export function buildCodexSettingsConfig(
  ctx: ProviderFormLogicContext,
  values: ProviderFormData,
): string {
  try {
    const authJson = JSON.parse(ctx.codex.codexAuth);
    let normalizedCodexConfig =
      ctx.category !== "official" && (ctx.codex.codexConfig ?? "").trim()
        ? setCodexWireApi(ctx.codex.codexConfig ?? "", "responses")
        : (ctx.codex.codexConfig ?? "");
    // 模型映射与「路由接管」解耦：对所有非官方供应商，填了就持久化
    //（Chat 生成兼容路由、原生 Responses 生成 model-catalogs.json），
    // 留空归一化为 [] 即不写。后端只看 modelCatalog.models 是否非空。
    const normalizedCatalogModels =
      ctx.category !== "official"
        ? normalizeCodexCatalogModelsForSave(ctx.codex.codexCatalogModels)
        : [];
    // The default-model field writes the top-level `model` into the TOML
    // as the user types; only when it was left empty fall back to the
    // first catalog row so "fill mapping only" keeps its old behavior.
    if (
      normalizedCatalogModels.length > 0 &&
      !extractCodexModelName(normalizedCodexConfig)
    ) {
      normalizedCodexConfig = setCodexModelNameInConfig(
        normalizedCodexConfig,
        normalizedCatalogModels[0].model,
      );
    }
    const configObj = {
      auth: authJson,
      config: normalizedCodexConfig,
    } as {
      auth: unknown;
      config: string;
      modelCatalog?: { models: CodexCatalogModel[] };
    };
    if (normalizedCatalogModels.length > 0) {
      configObj.modelCatalog = { models: normalizedCatalogModels };
    }
    return JSON.stringify(configObj);
  } catch (err) {
    return values.settingsConfig.trim();
  }
}

export function buildGeminiSettingsConfig(
  ctx: ProviderFormLogicContext,
  values: ProviderFormData,
): string {
  try {
    const envObj = ctx.gemini.envStringToObj(ctx.gemini.geminiEnv);
    const configObj = ctx.gemini.geminiConfig.trim()
      ? JSON.parse(ctx.gemini.geminiConfig)
      : {};
    const combined = {
      env: envObj,
      config: configObj,
    };
    return JSON.stringify(combined);
  } catch (err) {
    return values.settingsConfig.trim();
  }
}

export function buildOpenCodeSettingsConfig(
  ctx: ProviderFormLogicContext,
  values: ProviderFormData,
): string {
  if (ctx.category !== "omo" && ctx.category !== "omo-slim") {
    return values.settingsConfig.trim();
  }

  const omoConfig: Record<string, unknown> = {};
  if (Object.keys(ctx.opencode.omoAgents).length > 0) {
    omoConfig.agents = ctx.opencode.omoAgents;
  }
  if (
    ctx.category === "omo" &&
    Object.keys(ctx.opencode.omoCategories).length > 0
  ) {
    omoConfig.categories = ctx.opencode.omoCategories;
  }
  if (ctx.opencode.omoOtherFieldsStr.trim()) {
    // 格式已在 handleSubmit 前置校验中验证过，此处可以安全解析
    const otherFields = parseOmoOtherFieldsObject(
      ctx.opencode.omoOtherFieldsStr,
    );
    if (otherFields) {
      omoConfig.otherFields = otherFields;
    }
  }
  return JSON.stringify(omoConfig);
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
