import { fireEvent, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import App from './App';
import { renderWithI18n } from './test-utils';

vi.mock('./api/commands', () => ({
  api: {
    listProfiles: vi.fn().mockResolvedValue([]),
    listGames: vi.fn().mockResolvedValue([]),
    describeImage: vi.fn(),
    renderDocument: vi.fn(),
  },
  pickImage: vi.fn(),
  pickDocument: vi.fn(),
  errorMessage: (e: unknown) => String(e),
}));

describe('App', () => {
  it('starts on Generate and switches tabs accessibly', async () => {
    renderWithI18n(<App />);
    const generate = screen.getByRole('tab', { name: 'Generate' });
    const inject = screen.getByRole('tab', { name: 'Inject' });
    expect(generate).toHaveAttribute('aria-selected', 'true');
    fireEvent.click(inject);
    expect(inject).toHaveAttribute('aria-selected', 'true');
    expect(generate).toHaveAttribute('aria-selected', 'false');
    expect(await screen.findByRole('tabpanel', { name: 'Inject' })).toBeInTheDocument();
  });

  it('switches language', () => {
    renderWithI18n(<App />);
    fireEvent.change(screen.getByRole('combobox', { name: 'Language' }), { target: { value: 'fr' } });
    expect(screen.getByRole('tab', { name: 'Générer' })).toBeInTheDocument();
  });
});
