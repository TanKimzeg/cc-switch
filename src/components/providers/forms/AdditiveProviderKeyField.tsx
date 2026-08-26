// additive app（OpenCode / OpenClaw / Hermes）的 providerKey 主键输入字段。
// 三个 app 的渲染结构完全相同，仅 i18n key 与默认文案不同，由描述符的
// `providerKeyField` 槽位提供，因此这里收敛为一个共享组件。

import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import type { TFunction } from "i18next";
import type { ProviderKeyFieldConfig } from "./providerFormRegistry";

/** additive app 的 providerKey 状态（由各 app 表单 bundle 归一化而来） */
export interface AdditiveProviderKeyState {
  providerKey: string;
  onProviderKeyChange: (value: string) => void;
}

export interface AdditiveProviderKeyFieldProps extends ProviderKeyFieldConfig {
  providerKey: string;
  onProviderKeyChange: (value: string) => void;
  isProviderKeyLocked: boolean;
  isProviderKeyLockStateLoading: boolean;
  additiveExistingProviderKeys: string[];
  t: TFunction;
}

export function AdditiveProviderKeyField({
  i18n,
  providerKey,
  onProviderKeyChange,
  isProviderKeyLocked,
  isProviderKeyLockStateLoading,
  additiveExistingProviderKeys,
  t,
}: AdditiveProviderKeyFieldProps) {
  const providerKeyPattern = /^[a-z0-9]+(-[a-z0-9]+)*$/;
  const isDuplicate =
    additiveExistingProviderKeys.includes(providerKey) && !isProviderKeyLocked;
  const isInvalidFormat =
    providerKey.trim() !== "" && !providerKeyPattern.test(providerKey);
  const isValid =
    providerKey.trim() === "" || providerKeyPattern.test(providerKey);

  return (
    <div className="space-y-2">
      <Label htmlFor={i18n.fieldId}>
        {t(i18n.labelKey, { defaultValue: i18n.labelDefault })}
        <span className="text-destructive ml-1">*</span>
      </Label>
      <Input
        id={i18n.fieldId}
        value={providerKey}
        onChange={(e) =>
          onProviderKeyChange(
            e.target.value.toLowerCase().replace(/[^a-z0-9-]/g, ""),
          )
        }
        placeholder={t(i18n.placeholderKey, {
          defaultValue: i18n.placeholderDefault,
        })}
        disabled={isProviderKeyLocked || isProviderKeyLockStateLoading}
        className={isDuplicate || isInvalidFormat ? "border-destructive" : ""}
      />
      {isDuplicate && (
        <p className="text-xs text-destructive">{t(i18n.duplicateKey)}</p>
      )}
      {isInvalidFormat && (
        <p className="text-xs text-destructive">{t(i18n.invalidKey)}</p>
      )}
      {!isDuplicate && isValid && (
        <p className="text-xs text-muted-foreground">
          {isProviderKeyLocked
            ? t(i18n.lockedHintKey, { defaultValue: i18n.lockedHintDefault })
            : t(i18n.hintKey, { defaultValue: i18n.hintDefault })}
        </p>
      )}
    </div>
  );
}
