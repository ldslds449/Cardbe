// Keep these serialized values in sync with Rust's IrohDeviceStatus.
export enum IrohDeviceStatus {
  Pending = "pending",
  Approved = "approved",
  Revoked = "revoked",
}

export interface IrohDevice {
  node_id: string;
  status: IrohDeviceStatus;
}

export interface IrohInvite {
  invite_id: string;
  board_id: number;
  board_name: string;
  permission: "viewer" | "editor";
  enabled: boolean;
  created_at: string;
  devices: IrohDevice[];
}

export interface IrohConnectionInfo {
  observed_at: number;
  paths: Array<{ kind: "Direct" | "Relay" | "Custom"; address: string }>;
}

// Keep these serialized values in sync with Rust's IrohDeviceSyncFailure.
export type IrohDeviceSyncFailure = "unreachable" | "access_revoked" | "failed";

/** Latest owner-initiated sync with one device during this app session. */
export interface IrohDeviceSync {
  attempted_at: number;
  failure: IrohDeviceSyncFailure | null;
}

export interface IrohConnectionDetails {
  invitation: IrohConnectionInfo | null;
  devices: Record<string, IrohConnectionInfo>;
  boards: Record<number, IrohConnectionInfo>;
  device_syncs: Record<number, Record<string, IrohDeviceSync>>;
}
