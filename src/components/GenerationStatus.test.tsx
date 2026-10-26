import { fireEvent, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { initialGenerationState, type GenerationState } from '../hooks/useGeneration';
import { renderWithI18n } from '../test-utils';
import { GenerationStatus } from './GenerationStatus';

const running: GenerationState = { ...initialGenerationState, phase: 'running', shapes: 50, total: 100, rms: 12.345, startedAt: 0 };

describe('GenerationStatus', () => {
  it('renders nothing but warnings while idle', () => {
    const { container } = renderWithI18n(
      <GenerationStatus state={initialGenerationState} elapsedSeconds={0} onUseForInjection={vi.fn()} />,
    );
    expect(container).toBeEmptyDOMElement();
  });

  it('shows progress, error value, elapsed time and an estimate', () => {
    renderWithI18n(<GenerationStatus state={running} elapsedSeconds={100} onUseForInjection={vi.fn()} />);
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '50');
    expect(screen.getByText(/Error \(RMS\) 12\.35/)).toBeInTheDocument();
    expect(screen.getByText(/Elapsed 1:40/)).toBeInTheDocument();
    expect(screen.getByText(/~1:40 left/)).toBeInTheDocument();
  });

  it('says paused instead of the count and hides the estimate', () => {
    renderWithI18n(<GenerationStatus state={{ ...running, phase: 'paused' }} elapsedSeconds={100} onUseForInjection={vi.fn()} />);
    expect(screen.getByText('Paused')).toBeInTheDocument();
    expect(screen.queryByText(/left/)).not.toBeInTheDocument();
  });

  it('announces errors', () => {
    renderWithI18n(<GenerationStatus state={{ ...running, phase: 'error', error: 'out of memory' }} elapsedSeconds={0} onUseForInjection={vi.fn()} />);
    expect(screen.getByRole('alert')).toHaveTextContent('out of memory');
  });

  it('lists warnings', () => {
    renderWithI18n(<GenerationStatus state={{ ...running, warnings: ['disk full'] }} elapsedSeconds={0} onUseForInjection={vi.fn()} />);
    expect(screen.getByText('disk full')).toBeInTheDocument();
  });

  it('result card names the file and offers injection', () => {
    const onUse = vi.fn();
    const done: GenerationState = {
      ...running,
      phase: 'done',
      checkpoints: 3,
      summary: { path: '/out/pic.json', shapes: 100, rms: 9.5, stopped_early: false, message: null },
    };
    renderWithI18n(<GenerationStatus state={done} elapsedSeconds={0} onUseForInjection={onUse} />);
    expect(screen.getByText('100 shapes saved · RMS 9.50')).toBeInTheDocument();
    expect(screen.getByText('pic.json')).toBeInTheDocument();
    expect(screen.getByText(/3 checkpoints saved/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Use for injection' }));
    expect(onUse).toHaveBeenCalledOnce();
  });

  it('an early stop uses the early wording and shows the engine message', () => {
    const done: GenerationState = {
      ...running,
      phase: 'done',
      summary: { path: 'a.json', shapes: 40, rms: 20, stopped_early: true, message: 'nothing else fits' },
    };
    renderWithI18n(<GenerationStatus state={done} elapsedSeconds={0} onUseForInjection={vi.fn()} />);
    expect(screen.getByText('Stopped at 40 shapes · RMS 20.00')).toBeInTheDocument();
    expect(screen.getByText('nothing else fits')).toBeInTheDocument();
  });
});
