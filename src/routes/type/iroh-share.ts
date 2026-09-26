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
