// ProviderForm 渲染上下文（类型）：ProviderFormFull 组装一次上下文对象，
// 各 app 描述符的渲染函数（renderFormFields / renderConfigEditor）只从该抽象读取，
// 不再 `appId ===` 分支。类型字段与对应组件 props 保持一一对应，保证透传零转换。

import type { ReactNode } from "react";
import type { UseFormReturn } from "react-hook-form";
import type { TFunction } from "i18next";
import type { ProviderFormData } from "@/lib/schemas/provider";
import type { ProviderCategory } from "@/types";

/** Codex 配置编辑器所需状态（对应 CodexConfigEditorProps，减去表单通用字段） */
export interface CodexConfigEditorState {
  authValue: string;
  configValue: string;
  onAuthChange: (value: string) => void;
  onConfigChange: (value: string) => void;
  onAuthBlur?: () => void;
  useCommonConfig: boolean;
  onCommonConfigToggle: (checked: boolean) => void | Promise<void>;
  commonConfigSnippet: string;
  onCommonConfigSnippetChange: (value: string) => boolean | Promise<boolean>;
  onCommonConfigErrorClear: () => void;
  commonConfigError: string;
  authError: string;
  configError: string;
  onExtract?: () => void;
  isExtracting?: boolean;
}

/** Gemini 配置编辑器所需状态（对应 GeminiConfigEditorProps） */
export interface GeminiConfigEditorState {
  envValue: string;
  configValue: string;
  onEnvChange: (value: string) => void;
  onConfigChange: (value: string) => void;
  onEnvBlur?: () => void;
  useCommonConfig: boolean;
  onCommonConfigToggle: (checked: boolean) => void;
  commonConfigSnippet: string;
  onCommonConfigSnippetChange: (value: string) => boolean;
  onCommonConfigErrorClear: () => void;
  commonConfigError: string;
  envError: string;
  configError: string;
  onExtract?: () => void;
  isExtracting?: boolean;
}

/** Claude 通用配置编辑器所需状态（对应 CommonConfigEditorProps） */
export interface ClaudeConfigEditorState {
  useCommonConfig: boolean;
  onCommonConfigToggle: (checked: boolean) => void;
  commonConfigSnippet: string;
  onCommonConfigSnippetChange: (value: string) => void;
  commonConfigError: string;
  onEditClick: () => void;
  isModalOpen: boolean;
  onModalClose: () => void;
  onExtract?: () => void;
  isExtracting?: boolean;
}

export interface ProviderFormRenderContext {
  t: TFunction;
  form: UseFormReturn<ProviderFormData>;
  category?: ProviderCategory;
  isDarkMode: boolean;
  /** 代理接管模式（Codex 配置编辑器展示 Remote Compaction 需用） */
  isProxyTakeover: boolean;
  /** settingsConfig 的校验错误插槽 */
  settingsConfigErrorField: ReactNode;
  /** OMO 合并后的 JSON 预览（只读） */
  omoJsonPreview: string;
  /** 各 app 配置编辑器状态 */
  configEditor: {
    codex: CodexConfigEditorState;
    gemini: GeminiConfigEditorState;
    claude: ClaudeConfigEditorState;
  };
}
