export type DiagnosticInfo = {
  appVersion: string;
  buildCommit: string | null;
  os: string;
  architecture: string;
  logDirectory: string;
};

export function platformLabel(info: DiagnosticInfo): string {
  const os =
    {
      windows: "Windows",
      macos: "macOS",
      linux: "Linux",
      android: "Android",
      ios: "iOS",
    }[info.os] ?? info.os;
  return `${os} ${info.architecture}`;
}
