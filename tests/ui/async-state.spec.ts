import { expect, test } from "@playwright/test";

test("double activation and delayed status replies preserve the active stream", async ({ page }) => {
  await page.addInitScript(() => {
    let phase = "idle";
    let concurrentReads = 0;
    const fixture = {
      startCalls: 0,
      stopCalls: 0,
      statusReads: 0,
      maxConcurrentReads: 0,
      releaseStatus: () => {},
      invoke: async (command: string) => {
        switch (command) {
          case "discover_bridges": return [{ id: "bridge", name: "Test bridge", host: "192.168.1.42", port: 443 }];
          case "restore_bridge": return true;
          case "get_entertainment_areas": return [{ id: "area", name: "Bureau", channels: [{ channelId: 0, serviceId: "light", name: "Lampe", position: [-1, 0, 1] }] }];
          case "get_monitors": return [{ index: 0, name: "Moniteur", width: 1920, height: 1080, primary: true }];
          case "start_sync":
            fixture.startCalls++;
            await new Promise((resolve) => setTimeout(resolve, 120));
            phase = "running";
            return;
          case "stop_sync":
            if (++fixture.stopCalls === 1) { phase = "error"; throw new Error("Nettoyage du pont incomplet"); }
            phase = "idle";
            return;
          case "get_sync_status": {
            const snapshot = { running: phase === "running", phase, message: phase === "running" ? "Actif" : "Prêt", measuredFps: 45, frameTimeMs: 2, droppedFrames: 0, blackBarsDetected: false, colors: {} };
            fixture.statusReads++;
            fixture.maxConcurrentReads = Math.max(fixture.maxConcurrentReads, ++concurrentReads);
            if (fixture.statusReads === 1) await new Promise<void>((resolve) => { fixture.releaseStatus = resolve; });
            concurrentReads--;
            return snapshot;
          }
          case "log_frontend_event": return;
          default: throw new Error(`Unexpected command: ${command}`);
        }
      },
    };
    Object.assign(window, { __TAURI_INTERNALS__: fixture, testFixture: fixture });
  });
  await page.goto("/");
  await page.getByRole("button", { name: "Continuer", exact: true }).click();
  await expect.poll(() => page.evaluate(() => (window as any).testFixture.statusReads)).toBe(1);
  await page.getByRole("button", { name: "Démarrer l’éclairage", exact: true }).evaluate((button: HTMLButtonElement) => {
    button.click();
    button.click();
  });
  await expect(page.getByRole("button", { name: "Arrêter l’éclairage", exact: true })).toBeEnabled();
  await page.evaluate(() => (window as any).testFixture.releaseStatus());
  await expect.poll(() => page.evaluate(() => (window as any).testFixture.statusReads)).toBeGreaterThan(1);
  await expect(page.locator(".mode-chip")).toContainText("En direct");
  await expect(page.getByRole("slider", { name: "Luminosité", exact: true })).toBeDisabled();
  expect(await page.evaluate(() => (window as any).testFixture.startCalls)).toBe(1);
  expect(await page.evaluate(() => (window as any).testFixture.maxConcurrentReads)).toBe(1);
  await page.getByRole("button", { name: "Arrêter l’éclairage", exact: true }).click();
  await expect(page.getByRole("button", { name: "Réessayer l’arrêt", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Réessayer l’arrêt", exact: true }).click();
  await expect(page.getByRole("button", { name: "Démarrer l’éclairage", exact: true })).toBeEnabled();
  await expect(page.getByRole("button", { name: "Réessayer l’arrêt", exact: true })).toHaveCount(0);
});
