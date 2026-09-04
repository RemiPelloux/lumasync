export type MappingMode = "edges" | "corners";

export type Zone =
  | "left"
  | "right"
  | "top"
  | "bottom"
  | "topLeft"
  | "topRight"
  | "bottomLeft"
  | "bottomRight";

export interface BridgeInfo {
  id: string;
  host: string;
  name: string;
  port: number;
}

export interface LightChannel {
  channelId: number;
  serviceId: string;
  name: string;
  position: [number, number, number];
}

export interface EntertainmentArea {
  id: string;
  name: string;
  channels: LightChannel[];
}

export interface HueRoom {
  id: string;
  name: string;
  lightCount: number;
}

export interface MonitorInfo {
  index: number;
  name: string;
  width: number;
  height: number;
  primary: boolean;
}

export interface ChannelAssignment {
  channelId: number;
  zone: Zone;
}

export interface SyncSettings {
  brightness: number;
  saturation: number;
  reactivity: number;
  maxLuminosity: number;
  edgeDepth: number;
  fps: number;
  blackBarDetection: boolean;
}

export interface StartSyncRequest extends SyncSettings {
  areaId: string;
  monitorIndex: number;
  assignments: ChannelAssignment[];
}

export interface SyncStatus {
  running: boolean;
  phase: "idle" | "starting" | "running" | "reconnecting" | "stopping" | "error";
  message: string;
  measuredFps: number;
  frameTimeMs: number;
  droppedFrames: number;
  blackBarsDetected: boolean;
  colors: Record<string, string>;
}
