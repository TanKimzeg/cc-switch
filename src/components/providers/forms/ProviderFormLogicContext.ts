// ProviderForm 逻辑上下文（类型）：ProviderFormFull 组装一次上下文对象，
// 各 app 描述符的逻辑函数（applyPreset / resetCustom / buildSettingsConfig）
// 只从该抽象读取，不再 `appId ===` 分支。类型字段与各 app 表单状态保持一致。

import type { UseFormReturn } from "react-hook-form";
import type { TFunction } from "i18next";
import type { ProviderFormData } from "@/lib/schemas/provider";
import type {
  ProviderCategory,
  ClaudeApiFormat,
  ClaudeApiKeyField,
  CodexApiFormat,
  CodexCatalogModel,
  CodexChatReasoning,
  PromptCacheRoutingMode,
  OpenCodeProviderConfig,
  OpenClawProviderConfig,
} from "@/types";
import type { OpenClawSuggestedDefaults } from "@/config/openclawProviderPresets";
import type { HermesProviderSettingsConfig } from "@/config/hermesProviderPresets";

/** 当前选中预设（与 ProviderFormFull 内 useState 类型一致） */
export interface ActivePresetState {
  id: string;
  category?: ProviderCategory;
  isPartner?: boolean;
  partnerPromotionKey?: string;
  suggestedDefaults?: OpenClawSuggestedDefaults;
}

export interface ProviderFormLogicContext {
  t: TFunction;
  form: UseFormReturn<ProviderFormData>;
  /** 覆盖当前选中预设（OpenClaw 需追加 suggestedDefaults） */
  setActivePreset: (preset: ActivePresetState | null) => void;
  claude: {
    setLocalApiFormat: (format: ClaudeApiFormat) => void;
    setLocalApiKeyField: (field: ClaudeApiKeyField) => void;
    setLocalIsFullUrl: (value: boolean) => void;
  };
  codex: {
    resetCodexConfig: (
      auth: Record<string, unknown>,
      config: string,
      modelCatalogModels?: CodexCatalogModel[],
    ) => void;
    setCodexChatReasoning: (value: CodexChatReasoning) => void;
    setPromptCacheRouting: (value: PromptCacheRoutingMode) => void;
    setLocalCodexApiFormat: (value: CodexApiFormat) => void;
  };
  gemini: {
    resetGeminiConfig: (
      env: Record<string, unknown>,
      config: Record<string, unknown>,
    ) => void;
  };
  opencode: {
    resetOpencodeState: (config?: OpenCodeProviderConfig) => void;
    resetOmoDraftState: () => void;
  };
  openclaw: {
    resetOpenclawState: (config?: OpenClawProviderConfig) => void;
  };
  hermes: {
    resetHermesState: (config?: Partial<HermesProviderSettingsConfig>) => void;
  };
}
