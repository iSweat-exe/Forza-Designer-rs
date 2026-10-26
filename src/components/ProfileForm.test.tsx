import { fireEvent, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { makeProfile, renderWithI18n } from '../test-utils';
import { ProfileForm } from './ProfileForm';

const presets = [makeProfile({ name: 'fast', stop_at: 1500 }), makeProfile({ name: 'balanced' })];

function setup(over: Partial<React.ComponentProps<typeof ProfileForm>> = {}) {
  const props = {
    presets,
    presetName: 'balanced',
    draft: makeProfile(),
    disabled: false,
    onPreset: vi.fn(),
    onChange: vi.fn(),
    ...over,
  };
  renderWithI18n(<ProfileForm {...props} />);
  return props;
}

describe('ProfileForm', () => {
  it('marks the active preset and selects another on click', () => {
    const { onPreset } = setup();
    expect(screen.getByRole('radio', { name: 'Balanced' })).toHaveAttribute('aria-checked', 'true');
    fireEvent.click(screen.getByRole('radio', { name: 'Fast' }));
    expect(onPreset).toHaveBeenCalledWith('fast');
  });

  it('adds a shape type', () => {
    const { onChange } = setup();
    fireEvent.click(screen.getByLabelText('Circle'));
    expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ shape_types: ['rotated_ellipse', 'circle'] }));
  });

  it('never lets the last shape type be removed', () => {
    const { onChange } = setup();
    fireEvent.click(screen.getByLabelText('Rotated ellipse'));
    expect(onChange).not.toHaveBeenCalled();
  });

  it('removes a type while others remain', () => {
    const { onChange } = setup({ draft: makeProfile({ shape_types: ['rotated_ellipse', 'circle'] }) });
    fireEvent.click(screen.getByLabelText('Circle'));
    expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ shape_types: ['rotated_ellipse'] }));
  });

  it('clamps the shape count into its allowed range', () => {
    const { onChange } = setup();
    fireEvent.change(screen.getByLabelText('Number of shapes'), { target: { value: '99999' } });
    expect(onChange).toHaveBeenLastCalledWith(expect.objectContaining({ stop_at: 10000 }));
    fireEvent.change(screen.getByLabelText('Number of shapes'), { target: { value: '0' } });
    expect(onChange).toHaveBeenLastCalledWith(expect.objectContaining({ stop_at: 1 }));
  });

  it('ignores non-numeric input', () => {
    const { onChange } = setup();
    fireEvent.change(screen.getByLabelText('Number of shapes'), { target: { value: '' } });
    expect(onChange).not.toHaveBeenCalled();
  });

  it('disables every control while generating', () => {
    setup({ disabled: true });
    expect(screen.getByLabelText('Number of shapes')).toBeDisabled();
    expect(screen.getByRole('radio', { name: 'Fast' })).toBeDisabled();
  });
});
