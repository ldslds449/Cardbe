export const settingsSections = [
  "general",
  "theme",
  "notifications",
  "shortcuts",
  "sharing",
  "plugins",
  "mcp",
] as const;
export type SettingsSection = (typeof settingsSections)[number];

export function isSettingsSection(value: string): value is SettingsSection {
  return (settingsSections as readonly string[]).includes(value);
}

// One settings window for the whole app; any view can open it at a section.
export const settingsDialog = $state({
  open: false,
  section: "general" as SettingsSection,
  // Bumped after Iroh network settings are saved, so sharing views drop
  // invitations and access details created for the previous network.
  networkVersion: 0,
});

export function openSettings(section?: SettingsSection) {
  if (section) {
    settingsDialog.section = section;
  }
  settingsDialog.open = true;
}
