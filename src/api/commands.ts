import { Channel, invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';

import type {
  DocumentInfo,
  GameInfo,
  GenerateRequest,
  GenerationEvent,
  GenerationSummary,
  ImageInfo,
  InjectEvent,
  InjectReport,
  Profile,
} from './types';

/** Tauri rejects with a plain string for our commands. */
export function errorMessage(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  return String(e);
}

function channel<T>(onMessage: (m: T) => void): Channel<T> {
  const c = new Channel<T>();
  c.onmessage = onMessage;
  return c;
}

export const api = {
  listProfiles: () => invoke<Profile[]>('list_profiles'),
  listGames: () => invoke<GameInfo[]>('list_games'),
  describeImage: (path: string) => invoke<ImageInfo>('describe_image', { path }),

  generate: (request: GenerateRequest, onEvent: (e: GenerationEvent) => void) =>
    invoke<GenerationSummary>('generate', { request, onEvent: channel(onEvent) }),
  stopGeneration: () => invoke<void>('stop_generation'),
  setGenerationPaused: (paused: boolean) => invoke<void>('set_generation_paused', { paused }),

  loadDocument: (path: string) => invoke<DocumentInfo>('load_document', { path }),
  renderDocument: (path: string, maxSide: number) => invoke<string>('render_document', { path, maxSide }),

  inject: (path: string, game: string, onEvent: (e: InjectEvent) => void) =>
    invoke<InjectReport>('inject', { request: { path, game }, onEvent: channel(onEvent) }),
  cancelInjection: () => invoke<void>('cancel_injection'),
};

const IMAGE_EXTENSIONS = ['png', 'jpg', 'jpeg', 'bmp', 'webp', 'gif'];

export async function pickImage(title: string): Promise<string | null> {
  const picked = await open({
    title,
    multiple: false,
    filters: [{ name: 'Images', extensions: IMAGE_EXTENSIONS }],
  });
  return typeof picked === 'string' ? picked : null;
}

export async function pickDocument(title: string): Promise<string | null> {
  const picked = await open({ title, multiple: false, filters: [{ name: 'Forza Designer JSON', extensions: ['json'] }] });
  return typeof picked === 'string' ? picked : null;
}

