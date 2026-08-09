import { expect, it, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import App from "@/App";
import "@/i18n";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";

const renderApp = () => {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>,
  );
};

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

it("renders the shell title", async () => {
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === "get_plugins") return [];
    if (cmd === "get_providers") return [];
    return null;
  });
  renderApp();
  expect(await screen.findByText(/CC Switch v2/i)).toBeInTheDocument();
});

it("shows the empty plugin state when nothing is installed", async () => {
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === "get_plugins") return [];
    if (cmd === "get_providers") return [];
    return null;
  });
  renderApp();
  const headings = await screen.findAllByText("尚未安装插件");
  expect(headings.length).toBeGreaterThan(0);
});

it("lists installed plugins in the sidebar", async () => {
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === "get_plugins")
      return [
        {
          id: "openclaw",
          name: "OpenClaw",
          version: "0.1.0",
          apiVersion: "1",
          source: "builtin",
          installedAt: "2026-08-08",
        },
      ];
    if (cmd === "get_providers") return [];
    return null;
  });
  renderApp();
  expect(await screen.findByText("OpenClaw")).toBeInTheDocument();
});

it("shows live providers when the plugin supports readLive", async () => {
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === "get_plugins")
      return [
        {
          id: "opencode",
          name: "OpenCode",
          version: "0.1.0",
          apiVersion: "1",
          source: "builtin",
          installedAt: "2026-08-08",
          capabilities: { readLive: true, apply: true, import: true },
        },
      ];
    if (cmd === "get_providers") return [];
    if (cmd === "plugin_read_live")
      return {
        providers: [{ id: "openai", name: "OpenAI", settingsConfig: {} }],
        current: "openai",
      };
    return null;
  });
  renderApp();
  fireEvent.click(await screen.findByText("OpenCode"));
  await waitFor(() => {
    expect(screen.getByText("OpenAI")).toBeInTheDocument();
    expect(screen.getByText(/读取: ✓/)).toBeInTheDocument();
  });
});

it("applies a provider to the live config", async () => {
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === "get_plugins")
      return [
        {
          id: "opencode",
          name: "OpenCode",
          version: "0.1.0",
          apiVersion: "1",
          source: "builtin",
          installedAt: "2026-08-08",
          capabilities: { readLive: true, apply: true, import: true },
        },
      ];
    if (cmd === "get_providers")
      return [
        {
          id: "p1",
          pluginId: "opencode",
          name: "My Provider",
          category: "custom",
          sortOrder: 0,
          createdAt: "",
          updatedAt: "",
        },
      ];
    if (cmd === "plugin_read_live")
      return { providers: [], current: null };
    if (cmd === "plugin_apply") return null;
    return null;
  });
  renderApp();
  fireEvent.click(await screen.findByText("OpenCode"));
  await screen.findByText("My Provider");
  const applyBtn = screen.getByText("应用此供应商");
  fireEvent.click(applyBtn);
  await waitFor(() => {
    expect(invoke).toHaveBeenCalledWith("plugin_apply", expect.anything());
  });
});

it("shows MCP servers when the plugin supports mcp", async () => {
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === "get_plugins")
      return [
        {
          id: "opencode",
          name: "OpenCode",
          version: "0.1.0",
          apiVersion: "1",
          source: "builtin",
          installedAt: "2026-08-08",
          capabilities: { mcp: true },
        },
      ];
    if (cmd === "get_providers") return [];
    if (cmd === "plugin_mcp_get")
      return [
        { id: "filesystem", name: "Filesystem", spec: { type: "stdio", command: "npx" } },
      ];
    return null;
  });
  renderApp();
  fireEvent.click(await screen.findByText("OpenCode"));
  expect(await screen.findByText("Filesystem")).toBeInTheDocument();
});
