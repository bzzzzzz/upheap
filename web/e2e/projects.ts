// The widths the golden paths run at (design/app-ux.md, Testing), shared by both builds' configs: a wide screen, a
// tablet and a phone, each in Chromium, the one browser CI installs.
import { devices, type Project } from "@playwright/test";

export const projects: Project[] = [
  { name: "desktop", use: { ...devices["Desktop Chrome"] } },
  { name: "tablet", use: { ...devices["iPad (gen 7)"], browserName: "chromium" } },
  { name: "phone", use: { ...devices["Pixel 7"] } },
];
