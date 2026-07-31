// 供应商表单注册表（Plugin / Registry）
//
// 每个 app 在 [`PROVIDER_FORM_REGISTRY`] 中注册一份 [`ProviderFormAppDescriptor`]，
// ProviderForm / ProviderFormFull 只依赖该抽象，不再 `appId ===` match 具体 app，
// 从而让新增 app 只改本文件（开闭原则）。
//
// F1 阶段：先收敛纯数据（默认配置 / 预设 / 能力标志）；渲染与逻辑函数在 F1b/F1c
// 陆续迁入。

import type { AppId } from "@/lib/api/types";
import type { ReactNode } from "react";
import type { ProviderFormRenderContext } from "./ProviderFormRenderContext";
import {
  renderClaudeConfigEditor,
  renderCodexConfigEditor,
  renderGeminiConfigEditor,
  renderHermesConfigEditor,
  renderOpenclawConfigEditor,
  renderOpenCodeConfigEditor,
} from "./providerFormRender";
import {
  renderClaudeFormFields,
  renderCodexFormFields,
  renderGeminiFormFields,
  renderHermesFormFields,
  renderOpenclawFormFields,
  renderOpenCodeFormFields,
} from "./providerFormRender";
import {
  providerPresets,
  type ProviderPreset,
} from "@/config/claudeProviderPresets";
import {
  codexProviderPresets,
  type CodexProviderPreset,
} from "@/config/codexProviderPresets";
import {
  geminiProviderPresets,
  type GeminiProviderPreset,
} from "@/config/geminiProviderPresets";
import {
  opencodeProviderPresets,
  type OpenCodeProviderPreset,
} from "@/config/opencodeProviderPresets";
import {
  openclawProviderPresets,
  type OpenClawProviderPreset,
} from "@/config/openclawProviderPresets";
import {
  hermesProviderPresets,
  type HermesProviderPreset,
} from "@/config/hermesProviderPresets";
import {
  CLAUDE_DEFAULT_CONFIG,
  CLAUDE_DESKTOP_DEFAULT_CONFIG,
  CODEX_DEFAULT_CONFIG,
  GEMINI_DEFAULT_CONFIG,
  OPENCODE_DEFAULT_CONFIG,
  OPENCLAW_DEFAULT_CONFIG,
} from "./helpers/opencodeFormUtils";
import { HERMES_DEFAULT_CONFIG } from "./hooks/useHermesFormState";

export type ProviderFormPreset =
  | ProviderPreset
  | CodexProviderPreset
  | GeminiProviderPreset
  | OpenCodeProviderPreset
  | OpenClawProviderPreset
  | HermesProviderPreset;

export interface PresetEntry {
  id: string;
  preset: ProviderFormPreset;
}

/** additive app 的 providerKey 主键输入字段的 i18n 配置 */
export interface ProviderKeyFieldI18n {
  /** Input 的 htmlFor/id */
  fieldId: string;
  labelKey: string;
  labelDefault?: string;
  placeholderKey: string;
  placeholderDefault?: string;
  duplicateKey: string;
  invalidKey: string;
  requiredKey: string;
  /** 至少需要一个模型配置的软性校验（仅 opencode） */
  modelsRequiredKey?: string;
  hintKey: string;
  hintDefault?: string;
  lockedHintKey: string;
  lockedHintDefault?: string;
}

/** additive app 的 providerKey 主键字段槽位（渲染由共享 AdditiveProviderKeyField 承担） */
export interface ProviderKeyFieldConfig {
  i18n: ProviderKeyFieldI18n;
}

/** ProviderForm 各 app 的描述信息（纯数据；渲染与逻辑在 F1b/F1c 追加）。 */
export interface ProviderFormAppDescriptor {
  /** 与 `AppId` 一致 */
  appId: AppId;
  /** 预设 id 前缀（如 `claude-0`） */
  presetIdPrefix: string;
  /** 新建时默认的 settingsConfig 文本 */
  defaultSettingsConfig: string;
  /** 是否支持完整 URL（isFullUrl） */
  supportsFullUrl: boolean;
  /** 是否应用本地代理请求覆盖（非官方供应商） */
  supportsLocalProxyRequestOverrides: boolean;
  /** 是否显示价格/成本等高级配置 */
  supportsPricingConfig: boolean;
  /** 是否有预设模板变量（Claude 专属） */
  supportsTemplateValues: boolean;
  /** 是否 additive：providerKey 为主键，live 配置保留全部 provider */
  isAdditive: boolean;
  /** 是否支持 OMO 类目（OpenCode 专属） */
  hasOmoCategories: boolean;
  /** additive app 的 providerKey 主键字段（非 additive 无此槽位） */
  providerKeyField?: ProviderKeyFieldConfig;
  /** 配置编辑器渲染（ProviderFormFull 通过该槽位派发） */
  renderConfigEditor(ctx: ProviderFormRenderContext): ReactNode;
  /** 专属字段渲染（ProviderFormFull 通过该槽位派发） */
  renderFormFields(ctx: ProviderFormRenderContext): ReactNode;
  /** 构建该 app 的预设列表 */
  buildPresetEntries(): PresetEntry[];
}

export const PROVIDER_FORM_REGISTRY: Record<AppId, ProviderFormAppDescriptor> =
  {
    claude: {
      appId: "claude",
      presetIdPrefix: "claude",
      defaultSettingsConfig: CLAUDE_DEFAULT_CONFIG,
      supportsFullUrl: true,
      supportsLocalProxyRequestOverrides: true,
      supportsPricingConfig: true,
      supportsTemplateValues: true,
      isAdditive: false,
      hasOmoCategories: false,
      renderConfigEditor: (ctx) => renderClaudeConfigEditor(ctx),
      renderFormFields: (ctx) => renderClaudeFormFields(ctx),
      buildPresetEntries: () =>
        providerPresets
          .filter((p) => !p.hidden)
          .map<PresetEntry>((preset, index) => ({
            id: `claude-${index}`,
            preset,
          })),
    },
    "claude-desktop": {
      appId: "claude-desktop",
      presetIdPrefix: "claude-desktop",
      defaultSettingsConfig: CLAUDE_DESKTOP_DEFAULT_CONFIG,
      supportsFullUrl: false,
      supportsLocalProxyRequestOverrides: false,
      supportsPricingConfig: true,
      supportsTemplateValues: false,
      isAdditive: false,
      hasOmoCategories: false,
      renderConfigEditor: () => null,
      renderFormFields: () => null,
      buildPresetEntries: () => [],
    },
    codex: {
      appId: "codex",
      presetIdPrefix: "codex",
      defaultSettingsConfig: CODEX_DEFAULT_CONFIG,
      supportsFullUrl: true,
      supportsLocalProxyRequestOverrides: true,
      supportsPricingConfig: true,
      supportsTemplateValues: false,
      isAdditive: false,
      hasOmoCategories: false,
      renderConfigEditor: (ctx) => renderCodexConfigEditor(ctx),
      renderFormFields: (ctx) => renderCodexFormFields(ctx),
      buildPresetEntries: () =>
        codexProviderPresets.map<PresetEntry>((preset, index) => ({
          id: `codex-${index}`,
          preset,
        })),
    },
    gemini: {
      appId: "gemini",
      presetIdPrefix: "gemini",
      defaultSettingsConfig: GEMINI_DEFAULT_CONFIG,
      supportsFullUrl: false,
      supportsLocalProxyRequestOverrides: false,
      supportsPricingConfig: true,
      supportsTemplateValues: false,
      isAdditive: false,
      hasOmoCategories: false,
      renderConfigEditor: (ctx) => renderGeminiConfigEditor(ctx),
      renderFormFields: (ctx) => renderGeminiFormFields(ctx),
      buildPresetEntries: () =>
        geminiProviderPresets.map<PresetEntry>((preset, index) => ({
          id: `gemini-${index}`,
          preset,
        })),
    },
    grokbuild: {
      appId: "grokbuild",
      presetIdPrefix: "grokbuild",
      defaultSettingsConfig: CODEX_DEFAULT_CONFIG,
      supportsFullUrl: false,
      supportsLocalProxyRequestOverrides: false,
      supportsPricingConfig: true,
      supportsTemplateValues: false,
      isAdditive: false,
      hasOmoCategories: false,
      renderConfigEditor: () => null,
      renderFormFields: () => null,
      buildPresetEntries: () => [],
    },
    opencode: {
      appId: "opencode",
      presetIdPrefix: "opencode",
      defaultSettingsConfig: OPENCODE_DEFAULT_CONFIG,
      supportsFullUrl: false,
      supportsLocalProxyRequestOverrides: false,
      supportsPricingConfig: false,
      supportsTemplateValues: false,
      isAdditive: true,
      hasOmoCategories: true,
      providerKeyField: {
        i18n: {
          fieldId: "opencode-key",
          labelKey: "opencode.providerKey",
          placeholderKey: "opencode.providerKeyPlaceholder",
          duplicateKey: "opencode.providerKeyDuplicate",
          invalidKey: "opencode.providerKeyInvalid",
          requiredKey: "opencode.providerKeyRequired",
          modelsRequiredKey: "opencode.modelsRequired",
          hintKey: "opencode.providerKeyHint",
          lockedHintKey: "opencode.providerKeyLockedHint",
          lockedHintDefault: "该供应商已添加到应用配置中，供应商标识不可修改",
        },
      },
      renderConfigEditor: (ctx) => renderOpenCodeConfigEditor(ctx),
      renderFormFields: (ctx) => renderOpenCodeFormFields(ctx),
      buildPresetEntries: () =>
        opencodeProviderPresets.map<PresetEntry>((preset, index) => ({
          id: `opencode-${index}`,
          preset,
        })),
    },
    openclaw: {
      appId: "openclaw",
      presetIdPrefix: "openclaw",
      defaultSettingsConfig: OPENCLAW_DEFAULT_CONFIG,
      supportsFullUrl: false,
      supportsLocalProxyRequestOverrides: false,
      supportsPricingConfig: false,
      supportsTemplateValues: false,
      isAdditive: true,
      hasOmoCategories: false,
      providerKeyField: {
        i18n: {
          fieldId: "openclaw-key",
          labelKey: "openclaw.providerKey",
          placeholderKey: "openclaw.providerKeyPlaceholder",
          duplicateKey: "openclaw.providerKeyDuplicate",
          invalidKey: "openclaw.providerKeyInvalid",
          requiredKey: "openclaw.providerKeyRequired",
          hintKey: "openclaw.providerKeyHint",
          lockedHintKey: "openclaw.providerKeyLockedHint",
          lockedHintDefault: "该供应商已添加到应用配置中，供应商标识不可修改",
        },
      },
      renderConfigEditor: (ctx) => renderOpenclawConfigEditor(ctx),
      renderFormFields: (ctx) => renderOpenclawFormFields(ctx),
      buildPresetEntries: () =>
        openclawProviderPresets.map<PresetEntry>((preset, index) => ({
          id: `openclaw-${index}`,
          preset,
        })),
    },
    hermes: {
      appId: "hermes",
      presetIdPrefix: "hermes",
      defaultSettingsConfig: HERMES_DEFAULT_CONFIG,
      supportsFullUrl: false,
      supportsLocalProxyRequestOverrides: false,
      supportsPricingConfig: false,
      supportsTemplateValues: false,
      isAdditive: true,
      hasOmoCategories: false,
      providerKeyField: {
        i18n: {
          fieldId: "hermes-key",
          labelKey: "hermes.form.providerKey",
          labelDefault: "Provider Key",
          placeholderKey: "hermes.form.providerKeyPlaceholder",
          placeholderDefault: "my-provider",
          duplicateKey: "hermes.form.providerKeyDuplicate",
          invalidKey: "hermes.form.providerKeyInvalid",
          requiredKey: "hermes.form.providerKeyRequired",
          hintKey: "hermes.form.providerKeyHint",
          hintDefault:
            "Lowercase letters, numbers, and hyphens only. Used as the provider name in config.yaml.",
          lockedHintKey: "hermes.form.providerKeyLockedHint",
          lockedHintDefault:
            "This provider is in Hermes config; key is locked.",
        },
      },
      renderConfigEditor: (ctx) => renderHermesConfigEditor(ctx),
      renderFormFields: (ctx) => renderHermesFormFields(ctx),
      buildPresetEntries: () =>
        hermesProviderPresets.map<PresetEntry>((preset, index) => ({
          id: `hermes-${index}`,
          preset,
        })),
    },
  };

/** 获取指定 app 的表单描述；未注册时返回 undefined。 */
export function getProviderFormDescriptor(
  appId: AppId,
): ProviderFormAppDescriptor {
  return PROVIDER_FORM_REGISTRY[appId];
}
