import { fireEvent, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { renderWithI18n } from '../test-utils';
import { ImagePicker } from './ImagePicker';

const pickImage = vi.hoisted(() => vi.fn());
vi.mock('../api/commands', () => ({ pickImage }));

const info = { width: 480, height: 360, has_alpha: true, thumbnail: 'data:image/png;base64,AAA' };

describe('ImagePicker', () => {
  beforeEach(() => pickImage.mockReset());

  it('invites the user to pick an image when empty', () => {
    renderWithI18n(<ImagePicker path={null} info={null} disabled={false} onPath={vi.fn()} />);
    expect(screen.getByText(/Drop an image here/)).toBeInTheDocument();
  });

  it('shows the file name, size and transparency of the current image', () => {
    renderWithI18n(<ImagePicker path="C:\\pics\\cat.png" info={info} disabled={false} onPath={vi.fn()} />);
    expect(screen.getByText('cat.png')).toBeInTheDocument();
    expect(screen.getByText('480 × 360px · transparent')).toBeInTheDocument();
  });

  it('reports the path chosen in the dialog', async () => {
    pickImage.mockResolvedValue('/x/y.png');
    const onPath = vi.fn();
    renderWithI18n(<ImagePicker path={null} info={null} disabled={false} onPath={onPath} />);
    fireEvent.click(screen.getByRole('button'));
    await waitFor(() => expect(onPath).toHaveBeenCalledWith('/x/y.png'));
  });

  it('does nothing when the dialog is cancelled', async () => {
    pickImage.mockResolvedValue(null);
    const onPath = vi.fn();
    renderWithI18n(<ImagePicker path={null} info={null} disabled={false} onPath={onPath} />);
    fireEvent.click(screen.getByRole('button'));
    await waitFor(() => expect(pickImage).toHaveBeenCalled());
    expect(onPath).not.toHaveBeenCalled();
  });

  it('cannot be used while disabled', () => {
    renderWithI18n(<ImagePicker path={null} info={null} disabled onPath={vi.fn()} />);
    expect(screen.getByRole('button')).toBeDisabled();
  });
});
