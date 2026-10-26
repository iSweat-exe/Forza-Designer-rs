import { fireEvent, render, screen } from '@testing-library/react';
import { useState } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { ErrorBoundary } from './ErrorBoundary';

function Bomb({ explode }: { explode: boolean }) {
  if (explode) throw new Error('kaboom');
  return <p>fine</p>;
}

function Harness() {
  const [explode, setExplode] = useState(true);
  return (
    <ErrorBoundary
      fallback={(error, reset) => (
        <button
          onClick={() => {
            setExplode(false);
            reset();
          }}
        >
          {error.message}
        </button>
      )}
    >
      <Bomb explode={explode} />
    </ErrorBoundary>
  );
}

describe('ErrorBoundary', () => {
  afterEach(() => vi.restoreAllMocks());

  it('shows the fallback instead of crashing, then recovers on reset', () => {
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
    render(<Harness />);
    fireEvent.click(screen.getByRole('button', { name: 'kaboom' }));
    expect(screen.getByText('fine')).toBeInTheDocument();
  });

  it('renders children untouched when nothing throws', () => {
    render(
      <ErrorBoundary fallback={() => <p>fallback</p>}>
        <p>child</p>
      </ErrorBoundary>,
    );
    expect(screen.getByText('child')).toBeInTheDocument();
  });
});
