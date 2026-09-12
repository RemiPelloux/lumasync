import { expect, test } from "@playwright/test";

test("a malformed log timestamp does not crash the diagnostic viewer", async ({ page }) => {
  await page.addInitScript(() => {
    Object.assign(window, { __TAURI_INTERNALS__: { invoke: async (command: string) => {
      if (command === "get_diagnostics") return { logPath: null, entries: [{ timestampMs: 18446744073709551615, level: "warn", component: "storage", message: "Recovered log entry" }] };
      return [];
    } } });
  });
  await page.goto("/");
  await page.getByRole("button", { name: "Ouvrir le diagnostic", exact: true }).click();
  await expect(page.getByRole("dialog")).toContainText("Heure inconnue");
  await expect(page.locator(".app-shell")).toBeVisible();
});

test("runtime failures remain visible and diagnostic export includes context", async ({ page }) => {
  await page.addInitScript(() => {
    const entries: unknown[] = [];
    let phase = "idle";
    let reads = 0;
    const fixture = {
      failPolling: false,
      startCalls: 0,
      invoke: async (command: string, args: Record<string, any>) => {
        switch (command) {
          case "discover_bridges": return [{ id: "bridge", name: "Test bridge", host: "192.168.1.42", port: 443 }];
          case "restore_bridge": return true;
          case "get_entertainment_areas": return [{ id: "area", name: "Bureau", channels: [{ channelId: 0, serviceId: "light", name: "Lampe", position: [-1, 0, 1] }] }];
          case "get_monitors": return [{ index: 0, name: "Moniteur", width: 1920, height: 1080, primary: true }];
          case "start_sync": fixture.startCalls++; await new Promise((resolve) => setTimeout(resolve, 120)); phase = "running"; return;
          case "stop_sync": phase = "idle"; return;
          case "get_sync_status":
            if (fixture.failPolling) throw new Error("Lecture temporairement indisponible");
            if (phase === "running" && ++reads > 1) phase = "error";
            return { running: phase === "running", phase, message: phase === "error" ? "Capture perdue : écran débranché" : "Prêt", measuredFps: 0, frameTimeMs: 0, droppedFrames: 0, blackBarsDetected: false, colors: {} };
          case "log_frontend_event": entries.push({ timestampMs: Date.now(), level: args.level ?? "error", component: args.component, message: args.message }); return;
          case "get_diagnostics": return { entries, logPath: "C:\\LumaSync\\diagnostics.jsonl" };
          default: throw new Error(`Unexpected command: ${command}`);
        }
      },
    };
    Object.assign(window, { __TAURI_INTERNALS__: fixture, testFixture: fixture });
  });
  await page.goto("/");
  await page.getByRole("button", { name: "Continuer", exact: true }).click();
  await page.getByRole("button", { name: "Démarrer l’éclairage", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText("Capture perdue : écran débranché");
  await expect(page.locator(".mode-chip")).toContainText("Interrompu");
  await page.getByRole("button", { name: "Ouvrir le diagnostic", exact: true }).click();
  await expect(page.getByRole("dialog")).toContainText("Capture perdue");
  const download = page.waitForEvent("download");
  await page.getByRole("button", { name: "Exporter le journal", exact: true }).click();
  expect((await download).suggestedFilename()).toMatch(/^lumasync-diagnostic-.*\.jsonl$/);
  await page.keyboard.press("Escape");
  await page.evaluate(() => { (window as any).testFixture.failPolling = true; });
  await expect(page.locator(".app-alert--warning")).toContainText("suivi du flux est indisponible");
  await page.evaluate(() => { (window as any).testFixture.failPolling = false; });
  await page.getByRole("button", { name: "Réessayer la lecture du statut", exact: true }).click();
  await expect(page.locator(".app-alert--warning")).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).testFixture.startCalls)).toBe(1);
});
