import "@testing-library/jest-dom/vitest";

window.localStorage.setItem("language", "zh");

const i18n = await import("@/i18n");

export { i18n };
