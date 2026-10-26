// Mirrors the serde types in src-tauri/src/events.rs and crates/fd-core.

export type ShapeKind = 'ellipse' | 'rotated_ellipse' | 'circle' | 'rectangle' | 'rotated_rectangle';

export const SHAPE_KINDS: readonly ShapeKind[] = [
  'rotated_ellipse',
  'ellipse',
  'circle',
  'rotated_rectangle',
  'rectangle',
];

export interface Profile {
  name: string;
  description: string;
  max_preview_size: number;
  max_resolution: number;
  max_threads: number;
  mutated_samples: number;
  preview_every: number;
  random_samples: number;
  save_at: number[];
  save_every: number;
  stop_at: number;
  shape_types: ShapeKind[];
  shape_alpha: number;
}

export interface GameInfo {
  key: string;
  label: string;
  running: boolean;
  validated_build: string | null;
  injection_supported: boolean;
}

export interface ImageInfo {
  width: number;
  height: number;
  has_alpha: boolean;
  thumbnail: string;
}

export interface DocumentInfo {
  path: string;
  shape_count: number;
  image_size: [number, number];
  profile: string;
  source_image: string;
  sticker_mode: boolean;
  generated_at: string;
}

export interface GenerateRequest {
  imagePath: string;
  profile: Profile;
  stickerMode: boolean;
  outputDir?: string;
  seed?: number;
}

export type GenerationEvent =
  | { kind: 'started'; width: number; height: number }
  | { kind: 'progress'; shapes: number; total: number; rms: number }
  | { kind: 'preview'; png: string }
  | { kind: 'checkpoint'; path: string }
  | { kind: 'warning'; message: string };

export interface GenerationSummary {
  path: string;
  shapes: number;
  rms: number;
  stopped_early: boolean;
  message: string | null;
}

export type LocateMethod = 'sphere_scan' | 'rtti';

export type InjectEvent =
  | { kind: 'attached'; pid: number; build: string | null }
  | { kind: 'scanning'; layers: number }
  | { kind: 'scan_progress'; scanned: number; total: number; hits: number }
  | { kind: 'rtti_fallback' }
  /** 1 = class name, 2 = type descriptor refs, 3 = vtable refs, 4 = heap scan. */
  | { kind: 'rtti_phase'; phase: number }
  | { kind: 'located'; slots: number; method: LocateMethod }
  | { kind: 'writing'; written: number; total: number };

export interface InjectReport {
  slots: number;
  total: number;
  written: number;
  skipped: number;
  bytes: number;
  method: LocateMethod;
  type_counts: Record<string, number>;
  game_build: string | null;
  validated_build: string | null;
  overrides_applied: string[];
}
