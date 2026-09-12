import { expect, test } from "@playwright/test";

test("configuration, saved lamp placement, streaming and diagnostic", async ({ page }, info) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/");
  await page.getByRole("button", { name: "Continuer", exact: true }).click();
  await expect(page.getByRole("slider", { name: "Luminosité", exact: true })).toBeInViewport();
  await page.getByText("Position des lumières", { exact: false }).click();
  const firstLamp = page.getByRole("group", { name: "Position de Hue Play haut gauche", exact: true });
  await firstLamp.getByRole("button", { name: "Bas droit", exact: true }).click();
  await page.getByRole("button", { name: "Jeu", exact: false }).click();
  await page.reload();
  await page.getByRole("button", { name: "Continuer", exact: true }).click();
  await expect(page.getByRole("button", { name: "Jeu", exact: false })).toHaveAttribute("aria-pressed", "true");
  await page.getByText("Position des lumières", { exact: false }).click();
  await expect(firstLamp.getByRole("button", { name: "Bas droit", exact: true })).toHaveAttribute("aria-pressed", "true");
  await page.getByText("Position des lumières", { exact: false }).click();
  await page.getByRole("button", { name: "Démarrer l’éclairage", exact: true }).click();
  await expect(page.getByRole("slider", { name: "Luminosité", exact: true })).toBeDisabled();
  await expect(page.locator(".color-preview__zone")).toHaveCount(4);
  await expect(page.locator(".telemetry")).toContainText("59.8 i/s");
  const before = await page.locator(".color-preview__zone--topRight").getAttribute("style");
  await expect.poll(() => page.locator(".color-preview__zone--topRight").getAttribute("style")).not.toBe(before);
  await page.screenshot({ path: info.outputPath("desktop-streaming.png") });
  await page.getByRole("button", { name: "Arrêter l’éclairage", exact: true }).click();
  await expect(page.getByRole("slider", { name: "Luminosité", exact: true })).toBeEnabled();
  await page.getByRole("button", { name: "Ouvrir le diagnostic", exact: true }).click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  expect(errors).toEqual([]);
});

test("small windows retain all controls without horizontal overflow", async ({ page }, info) => {
  for (const width of [320, 390, 860]) {
    await page.setViewportSize({ width, height: 844 });
    await page.goto("/");
    await page.getByRole("button", { name: "Continuer", exact: true }).click();
    await page.getByRole("button", { name: "Démarrer l’éclairage", exact: true }).click();
    await expect(page.locator(".color-preview__zone")).toHaveCount(4);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
    const overlaps = await page.evaluate(() => {
      const scene = document.querySelector(".monitor-frame")!.getBoundingClientRect();
      const footer = document.querySelector(".stage__footer")!.getBoundingClientRect();
      return scene.bottom > footer.top;
    });
    expect(overlaps).toBe(false);
    await page.screenshot({ path: info.outputPath(`window-${width}.png`), fullPage: true });
    await page.getByRole("button", { name: "Arrêter l’éclairage", exact: true }).click();
  }
});

test("an empty Entertainment list can recover through a Hue room", async ({ page }) => {
  await page.goto("/?empty");
  await page.getByRole("button", { name: "Continuer", exact: true }).click();
  await expect(page.getByRole("button", { name: "Démarrer l’éclairage", exact: true })).toBeDisabled();
  await page.getByRole("button", { name: "Créer depuis Chambre", exact: true }).click();
  await expect(page.getByRole("button", { name: "Démarrer l’éclairage", exact: true })).toBeEnabled();
});
