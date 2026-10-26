import { fireEvent, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { DocumentInfo, InjectEvent, InjectReport } from '../api/types';
import { renderWithI18n } from '../test-utils';
import { InjectTab } from './InjectTab';

const mocks = vi.hoisted(() => ({
  api: { listGames: vi.fn(), renderDocument: vi.fn(), loadDocument: vi.fn(), inject: vi.fn(), cancelInjection: vi.fn() },
  pickDocument: vi.fn(),
}));
vi.mock('../api/commands', () => ({ api: mocks.api, pickDocument: mocks.pickDocument, errorMessage: (e: unknown) => String(e) }));

const game = (over = {}) => ({ key: 'fh6', label: 'Forza Horizon 6', running: true, validated_build: '364.933', injection_supported: true, ...over });
const doc: DocumentInfo = { path: '/o/pic.json', shape_count: 40, image_size: [556, 556], profile: 'balanced', source_image: 'pic.png', sticker_mode: false, generated_at: '' };
const report = (over: Partial<InjectReport> = {}): InjectReport => ({
  slots: 500, total: 40, written: 40, skipped: 0, bytes: 1, method: 'sphere_scan', type_counts: { rotated_ellipse: 40 },
  game_build: '364.933', validated_build: '364.933', overrides_applied: [], ...over,
});

describe('InjectTab', () => {
  beforeEach(() => {
    Object.values(mocks.api).forEach((f) => f.mockReset());
    mocks.api.listGames.mockResolvedValue([game()]);
    mocks.api.renderDocument.mockResolvedValue('data:image/png;base64,AAA');
    mocks.api.cancelInjection.mockResolvedValue(undefined);
  });

  it('asks for a document first and keeps Inject disabled', async () => {
    renderWithI18n(<InjectTab document={null} onDocument={vi.fn()} />);
    expect(await screen.findByText('running')).toBeInTheDocument();
    expect(screen.getAllByText(/Generate an image first/).length).toBeGreaterThan(0);
    expect(screen.getByRole('button', { name: 'Inject into Forza Horizon 6' })).toBeDisabled();
  });

  it('warns that injection is Windows-only and blocks the button', async () => {
    mocks.api.listGames.mockResolvedValue([game({ injection_supported: false, running: false })]);
    renderWithI18n(<InjectTab document={doc} onDocument={vi.fn()} />);
    expect(await screen.findByText(/only available on Windows/)).toBeInTheDocument();
    expect(screen.getByText('not running')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Inject into Forza Horizon 6' })).toBeDisabled();
  });

  it('tells the user how many layers the template needs', async () => {
    renderWithI18n(<InjectTab document={doc} onDocument={vi.fn()} />);
    expect(await screen.findByText(/at least 40 layers/)).toBeInTheDocument();
  });

  it('loads a document chosen from disk', async () => {
    mocks.pickDocument.mockResolvedValue('/x/other.json');
    mocks.api.loadDocument.mockResolvedValue({ ...doc, path: '/x/other.json' });
    const onDocument = vi.fn();
    renderWithI18n(<InjectTab document={null} onDocument={onDocument} />);
    fireEvent.click(await screen.findByRole('button', { name: 'Choose a JSON file' }));
    await waitFor(() => expect(onDocument).toHaveBeenCalledWith(expect.objectContaining({ path: '/x/other.json' })));
  });

  it('shows why a file could not be loaded', async () => {
    mocks.pickDocument.mockResolvedValue('/x/bad.json');
    mocks.api.loadDocument.mockRejectedValue('unsupported document format');
    renderWithI18n(<InjectTab document={null} onDocument={vi.fn()} />);
    fireEvent.click(await screen.findByRole('button', { name: 'Choose a JSON file' }));
    expect(await screen.findByText(/unsupported document format/)).toBeInTheDocument();
  });

  it('walks through scanning, writing and the final report', async () => {
    mocks.api.inject.mockImplementation(async (_p: string, _g: string, onEvent: (e: InjectEvent) => void) => {
      onEvent({ kind: 'attached', pid: 4242, build: '364.933' });
      onEvent({ kind: 'scanning', layers: 40 });
      onEvent({ kind: 'located', slots: 500, method: 'sphere_scan' });
      onEvent({ kind: 'writing', written: 40, total: 40 });
      return report();
    });
    renderWithI18n(<InjectTab document={doc} onDocument={vi.fn()} />);
    fireEvent.click(await screen.findByRole('button', { name: 'Inject into Forza Horizon 6' }));
    expect(await screen.findByText('Injected 40 of 40 shapes.')).toBeInTheDocument();
    expect(mocks.api.inject).toHaveBeenCalledWith('/o/pic.json', 'fh6', expect.any(Function));
    expect(screen.getByText(/Attached to the game \(pid 4242\), build 364.933/)).toBeInTheDocument();
    expect(screen.getByText('Found a vinyl group with 500 layer slots.')).toBeInTheDocument();
  });

  it('flags skipped layers and a build mismatch', async () => {
    mocks.api.inject.mockResolvedValue(report({ written: 38, skipped: 2, game_build: '364.934' }));
    renderWithI18n(<InjectTab document={doc} onDocument={vi.fn()} />);
    fireEvent.click(await screen.findByRole('button', { name: 'Inject into Forza Horizon 6' }));
    expect(await screen.findByText(/2 layers failed validation/)).toBeInTheDocument();
    expect(screen.getByText(/364\.934 differs from the validated build 364\.933/)).toBeInTheDocument();
  });

  it('shows the long RTTI fallback warning', async () => {
    let finish!: (r: InjectReport) => void;
    mocks.api.inject.mockImplementation((_p: string, _g: string, onEvent: (e: InjectEvent) => void) => {
      onEvent({ kind: 'rtti_fallback' });
      onEvent({ kind: 'rtti_phase', phase: 3 });
      return new Promise<InjectReport>((r) => (finish = r));
    });
    renderWithI18n(<InjectTab document={doc} onDocument={vi.fn()} />);
    fireEvent.click(await screen.findByRole('button', { name: 'Inject into Forza Horizon 6' }));
    expect(await screen.findByText(/slower type scan/)).toBeInTheDocument();
    expect(screen.getByText(/Type scan 3\/4/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(mocks.api.cancelInjection).toHaveBeenCalled();
    finish(report());
  });

  it('reports a failed injection as an alert', async () => {
    mocks.api.inject.mockRejectedValue('Run this app as Administrator');
    renderWithI18n(<InjectTab document={doc} onDocument={vi.fn()} />);
    fireEvent.click(await screen.findByRole('button', { name: 'Inject into Forza Horizon 6' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Run this app as Administrator');
  });
});
