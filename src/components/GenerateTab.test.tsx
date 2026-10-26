import { fireEvent, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { GenerationEvent } from '../api/types';
import { makeProfile, renderWithI18n } from '../test-utils';
import { GenerateTab } from './GenerateTab';

const mocks = vi.hoisted(() => ({
  api: {
    listProfiles: vi.fn(),
    describeImage: vi.fn(),
    generate: vi.fn(),
    stopGeneration: vi.fn(),
    setGenerationPaused: vi.fn(),
    loadDocument: vi.fn(),
  },
  pickImage: vi.fn(),
}));
vi.mock('../api/commands', () => ({ api: mocks.api, pickImage: mocks.pickImage, errorMessage: (e: unknown) => String(e) }));

const info = { width: 100, height: 80, has_alpha: false, thumbnail: 'data:image/png;base64,AAA' };
const summary = { path: '/o/pic.json', shapes: 20, rms: 11, stopped_early: false, message: null };
const doc = { path: '/o/pic.json', shape_count: 20, image_size: [10, 10] as [number, number], profile: 'balanced', source_image: 'pic.png', sticker_mode: false, generated_at: '' };

async function pickImage() {
  mocks.pickImage.mockResolvedValue('/img/pic.png');
  fireEvent.click(await screen.findByRole('button', { name: 'Choose an image' }));
  await screen.findByText('pic.png');
}

describe('GenerateTab', () => {
  beforeEach(() => {
    Object.values(mocks.api).forEach((f) => f.mockReset());
    mocks.api.listProfiles.mockResolvedValue([makeProfile({ name: 'fast', stop_at: 1500 }), makeProfile({ name: 'balanced' })]);
    mocks.api.describeImage.mockResolvedValue(info);
    mocks.api.stopGeneration.mockResolvedValue(undefined);
    mocks.api.setGenerationPaused.mockResolvedValue(undefined);
  });

  it('cannot start before an image is chosen', async () => {
    renderWithI18n(<GenerateTab onDocument={vi.fn()} onGoInject={vi.fn()} />);
    expect(await screen.findByRole('radio', { name: 'Balanced' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Start' })).toBeDisabled();
  });

  it('runs a generation with the edited profile and hands the document up', async () => {
    mocks.api.generate.mockImplementation(async (_r: unknown, onEvent: (e: GenerationEvent) => void) => {
      onEvent({ kind: 'progress', shapes: 5, total: 100, rms: 40 });
      return summary;
    });
    mocks.api.loadDocument.mockResolvedValue(doc);
    const onDocument = vi.fn();
    renderWithI18n(<GenerateTab onDocument={onDocument} onGoInject={vi.fn()} />);

    await pickImage();
    fireEvent.change(await screen.findByLabelText('Number of shapes'), { target: { value: '100' } });
    fireEvent.click(screen.getByLabelText(/Sticker mode/));
    fireEvent.click(screen.getByRole('button', { name: 'Start' }));

    await waitFor(() => expect(onDocument).toHaveBeenCalledWith(doc));
    const request = mocks.api.generate.mock.calls[0]?.[0];
    expect(request).toMatchObject({ imagePath: '/img/pic.png', stickerMode: true, profile: { stop_at: 100 } });
    expect(screen.getByText('20 shapes saved · RMS 11.00')).toBeInTheDocument();
  });

  it('shows pause and stop while running and forwards them', async () => {
    mocks.api.generate.mockReturnValue(new Promise(() => undefined));
    renderWithI18n(<GenerateTab onDocument={vi.fn()} onGoInject={vi.fn()} />);
    await pickImage();
    fireEvent.click(await screen.findByRole('button', { name: 'Start' }));

    fireEvent.click(await screen.findByRole('button', { name: 'Pause' }));
    await waitFor(() => expect(mocks.api.setGenerationPaused).toHaveBeenCalledWith(true));
    expect(await screen.findByRole('button', { name: 'Resume' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Stop' }));
    expect(mocks.api.stopGeneration).toHaveBeenCalled();
    // Settings are locked during a run.
    expect(screen.getByLabelText('Number of shapes')).toBeDisabled();
  });

  it('shows a failed run as an alert and allows another try', async () => {
    mocks.api.generate.mockRejectedValue('could not decode image');
    renderWithI18n(<GenerateTab onDocument={vi.fn()} onGoInject={vi.fn()} />);
    await pickImage();
    fireEvent.click(await screen.findByRole('button', { name: 'Start' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('could not decode image');
    expect(screen.getByRole('button', { name: 'Start' })).toBeEnabled();
  });

  it('reports an unreadable image without enabling Start', async () => {
    mocks.api.describeImage.mockRejectedValue('not an image');
    renderWithI18n(<GenerateTab onDocument={vi.fn()} onGoInject={vi.fn()} />);
    mocks.pickImage.mockResolvedValue('/img/bad.txt');
    fireEvent.click(await screen.findByRole('button', { name: 'Choose an image' }));
    expect(await screen.findByText('not an image')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Start' })).toBeDisabled();
  });

  it('switching preset resets the edits', async () => {
    renderWithI18n(<GenerateTab onDocument={vi.fn()} onGoInject={vi.fn()} />);
    const input = await screen.findByLabelText('Number of shapes');
    fireEvent.change(input, { target: { value: '7' } });
    expect(input).toHaveValue(7);
    fireEvent.click(screen.getByRole('radio', { name: 'Fast' }));
    expect(screen.getByLabelText('Number of shapes')).toHaveValue(1500);
  });
});
