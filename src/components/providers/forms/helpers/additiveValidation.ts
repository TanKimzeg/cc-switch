import type { TFunction } from "i18next";
import { toast } from "sonner";
import type { ProviderKeyFieldI18n } from "../providerFormRegistry";

const ADDITIVE_PROVIDER_KEY_PATTERN = /^[a-z0-9]+(-[a-z0-9]+)*$/;

export interface ValidateAdditiveProviderKeyParams {
  providerKey: string;
  i18n: ProviderKeyFieldI18n;
  isProviderKeyLockStateLoading: boolean;
  isProviderKeyLocked: boolean;
  additiveExistingProviderKeys: string[];
  t: TFunction;
}

/**
 * 校验 additive app 的 providerKey 主键（opencode / openclaw / hermes 共用）。
 * providerKey 是这些 app 的主键 ID，空 / 格式不合法 / 重复 / 状态加载中都属于
 * 完整性约束，保留硬拒绝（mutations 层也会 throw，软化只会让错误更晦涩）。
 * 返回 true 表示命中硬错误，调用方应立即 return。
 */
export function validateAdditiveProviderKey({
  providerKey,
  i18n,
  isProviderKeyLockStateLoading,
  isProviderKeyLocked,
  additiveExistingProviderKeys,
  t,
}: ValidateAdditiveProviderKeyParams): boolean {
  if (!providerKey.trim()) {
    toast.error(t(i18n.requiredKey));
    return true;
  }
  if (!ADDITIVE_PROVIDER_KEY_PATTERN.test(providerKey)) {
    toast.error(t(i18n.invalidKey));
    return true;
  }
  if (isProviderKeyLockStateLoading) {
    toast.error(
      t("providerForm.providerKeyStatusLoading", {
        defaultValue: "正在加载供应商标识状态，请稍后再试",
      }),
    );
    return true;
  }
  if (
    !isProviderKeyLocked &&
    additiveExistingProviderKeys.includes(providerKey)
  ) {
    toast.error(t(i18n.duplicateKey));
    return true;
  }
  return false;
}
