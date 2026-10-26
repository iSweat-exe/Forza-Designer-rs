import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { ProgressBar } from './ProgressBar';

describe('ProgressBar', () => {
  it('exposes progress to assistive tech', () => {
    render(<ProgressBar value={25} max={100} label="Shapes" />);
    const bar = screen.getByRole('progressbar', { name: 'Shapes' });
    expect(bar).toHaveAttribute('aria-valuenow', '25');
    expect(bar).toHaveAttribute('aria-valuemax', '100');
  });

  it('fills proportionally and never beyond 100 %', () => {
    const { container, rerender } = render(<ProgressBar value={50} max={200} />);
    expect(container.querySelector<HTMLElement>('.progress-fill')?.style.width).toBe('25%');
    rerender(<ProgressBar value={500} max={200} />);
    expect(container.querySelector<HTMLElement>('.progress-fill')?.style.width).toBe('100%');
  });

  it('shows 0 % for an unknown maximum instead of dividing by zero', () => {
    const { container } = render(<ProgressBar value={3} max={0} />);
    expect(container.querySelector<HTMLElement>('.progress-fill')?.style.width).toBe('0%');
  });

  it('indeterminate mode has no value and no width', () => {
    const { container } = render(<ProgressBar value={0} max={0} indeterminate />);
    const bar = screen.getByRole('progressbar');
    expect(bar).not.toHaveAttribute('aria-valuenow');
    expect(bar).toHaveClass('indeterminate');
    expect(container.querySelector<HTMLElement>('.progress-fill')?.style.width).toBe('');
  });
});
