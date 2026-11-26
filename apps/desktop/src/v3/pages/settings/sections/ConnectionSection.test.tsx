// apps/desktop/src/v3/pages/settings/sections/ConnectionSection.test.tsx
// D-RN-ONBOARDING-2: Settings → Connection reads the shell's stored connection (never the token)
// and writes the same config keys onboarding writes. The round-trip test runs the real onboarding
// page against one stateful config map, then checks what Settings shows and writes back.
import { act, fireEvent, render, screen } from '@testing-library/react';
import type { WarshaClient } from '@warsha/client';
import { I18nProvider } from '@warsha/i18n/v3';
import React from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CapabilitiesProvider } from '../../../app/CapabilitiesContext';
import { ClientProvider } from '../../../app/ClientContext';
import { DataReadyProvider } from '../../../app/DataReadyContext';
import { DensityProvider } from '../../../app/DensityContext';
import { ThemeProvider } from '../../../app/ThemeContext';
import { RouterProvider } from '../../../router';
import OnboardingPage from '../../onboarding';

import { ConnectionSection } from './ConnectionSection';

const ONBOARDING_URL = 'http://warsha.example:7878';
const ONBOARDING_TOKEN = 'tok-onboarding-server';

interface ShellCall {
  cmd: string;
  args?: Record<string, unknown>;
}

/** One in-memory config map plus a swappable `test_daemon_connection` reply, as the shell would hold. */
const shell: {
  config: Map<string, string>;
  connectionReply: { ok: boolean; reason?: string; detail: string; wsUrl: string; version?: string };
  calls: ShellCall[];
} = {
  config: new Map<string, string>(),
  connectionReply: { ok: false, reason: 'unreachable', detail: '', wsUrl: '' },
  calls: [],
};

function installShellStub(): void {
  (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
    invoke: (cmd: string, args?: Record<string, unknown>) => {
      shell.calls.push({ cmd, args });
      if (cmd === 'get_config') {
        const key = typeof args?.key === 'string' ? args.key : '';
        return Promise.resolve(shell.config.get(key) ?? '');
      }
      if (cmd === 'set_config') {
        const key = typeof args?.key === 'string' ? args.key : '';
        const value = typeof args?.value === 'string' ? args.value : '';
        shell.config.set(key, value);
        return Promise.resolve('');
      }
      if (cmd === 'test_daemon_connection') {
        return Promise.resolve(shell.connectionReply);
      }
      return Promise.resolve('');
    },
    transformCallback: () => 1,
    unregisterCallback: () => undefined,
    convertFileSrc: (filePath: string) => filePath,
  };
}

async function settle(): Promise<void> {
  await act(async () => {
    for (let i = 0; i < 12; i += 1) {
      await Promise.resolve();
    }
  });
}

function makeClient(): WarshaClient {
  return {
    health: vi.fn().mockResolvedValue({ status: 'ok', version: '0.2.1' }),
    listProviders: vi.fn().mockResolvedValue([]),
    listPreferences: vi.fn().mockResolvedValue([]),
    listCapabilities: vi.fn().mockResolvedValue([]),
  } as unknown as WarshaClient;
}

function renderSection(client: WarshaClient = makeClient()): ReturnType<typeof render> {
  return render(
    <ClientProvider client={client}>
      <CapabilitiesProvider client={client}>
        <I18nProvider initialLocale="en">
          <DataReadyProvider>
            <DensityProvider>
              <ThemeProvider>
                <ConnectionSection />
              </ThemeProvider>
            </DensityProvider>
          </DataReadyProvider>
        </I18nProvider>
      </CapabilitiesProvider>
    </ClientProvider>
  );
}

async function renderOnboarding(client: WarshaClient = makeClient()): Promise<ReturnType<typeof render>> {
  let utils!: ReturnType<typeof render>;
  await act(async () => {
    utils = render(
      <ClientProvider client={client}>
        <CapabilitiesProvider client={client}>
          <I18nProvider initialLocale="en">
            <DataReadyProvider>
              <RouterProvider initialPath="/onboarding">
                <OnboardingPage />
              </RouterProvider>
            </DataReadyProvider>
          </I18nProvider>
        </CapabilitiesProvider>
      </ClientProvider>
    );
    await Promise.resolve();
  });
  return utils;
}

function fieldInput(testId: string): HTMLInputElement {
  const input = screen.getByTestId(testId).querySelector('input');
  if (!input) throw new Error(`No input inside ${testId}`);
  return input as HTMLInputElement;
}

describe('ConnectionSection', () => {
  beforeEach(() => {
    window.location.hash = '';
    try {
      window.localStorage?.clear();
    } catch {
      // ignore
    }
    shell.config.clear();
    shell.calls.length = 0;
    shell.connectionReply = { ok: false, reason: 'unreachable', detail: '', wsUrl: '' };
    installShellStub();
  });

  afterEach(() => {
    delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  });

  it('shows this device by default and writes connection_mode=local when saved', async () => {
    await act(async () => {
      renderSection();
      await Promise.resolve();
    });

    expect(screen.getByTestId('settings-section-connection')).toBeTruthy();
    expect(screen.getByTestId('settings-connection-mode').textContent).toBe(
      'Warsha runs on this device'
    );
    expect(screen.queryByTestId('settings-connection-address')).toBeNull();

    fireEvent.click(screen.getByTestId('settings-connection-save'));
    await settle();

    expect(shell.config.get('connection_mode')).toBe('local');
    expect(screen.getByTestId('settings-connection-saved')).toBeTruthy();
  });

  it('never prints the stored token, only that one is set', async () => {
    shell.config.set('connection_mode', 'server');
    shell.config.set('daemon_url', ONBOARDING_URL);
    shell.config.set('daemon_token', ONBOARDING_TOKEN);

    await act(async () => {
      renderSection();
      await Promise.resolve();
    });
    await settle();

    expect(screen.getByTestId('settings-connection-mode').textContent).toBe(
      'Connected to a Warsha server'
    );
    expect(screen.getByTestId('settings-connection-address').textContent).toBe(ONBOARDING_URL);
    expect(screen.getByTestId('settings-connection-token-state').textContent).toBe(
      'A token is saved for this server'
    );
    expect(screen.getByTestId('settings-section-connection').textContent).not.toContain(
      ONBOARDING_TOKEN
    );
    // The token field itself starts empty even when one is stored.
    expect(fieldInput('settings-connection-token').value).toBe('');
  });

  it('will not save a server until its own test passes', async () => {
    await act(async () => {
      renderSection();
      await Promise.resolve();
    });

    fireEvent.click(screen.getByTestId('settings-connection-mode-server'));
    await settle();
    fireEvent.change(fieldInput('settings-connection-url'), {
      target: { value: 'http://10.0.0.9:7878' },
    });
    await settle();

    expect((screen.getByTestId('settings-connection-save') as HTMLButtonElement).disabled).toBe(
      true
    );
    expect(shell.config.get('connection_mode')).toBeUndefined();

    shell.connectionReply = { ok: true, detail: 'ok', wsUrl: 'ws://10.0.0.9:7878/ws' };
    fireEvent.click(screen.getByTestId('settings-connection-test'));
    await settle();
    expect((screen.getByTestId('settings-connection-save') as HTMLButtonElement).disabled).toBe(
      false
    );

    fireEvent.click(screen.getByTestId('settings-connection-save'));
    await settle();
    expect(shell.config.get('connection_mode')).toBe('server');
    expect(shell.config.get('daemon_url')).toBe('http://10.0.0.9:7878');
    expect(shell.config.get('daemon_ws_url')).toBe('ws://10.0.0.9:7878/ws');
  });

  it('round-trips the server chosen in onboarding back to this device', async () => {
    shell.connectionReply = {
      ok: true,
      detail: 'ok',
      wsUrl: 'ws://warsha.example:7878/ws',
      version: '0.9.3',
    };

    const onboarding = await renderOnboarding();

    // language -> connection
    fireEvent.click(screen.getByTestId('onboarding-next-btn'));
    await settle();
    expect(screen.getByTestId('onboarding-step-connection')).toBeTruthy();

    fireEvent.click(screen.getByTestId('onboarding-connection-mode-server'));
    await settle();
    fireEvent.change(fieldInput('onboarding-connection-url'), {
      target: { value: ONBOARDING_URL },
    });
    fireEvent.change(fieldInput('onboarding-connection-token'), {
      target: { value: ONBOARDING_TOKEN },
    });
    await settle();
    fireEvent.click(screen.getByTestId('onboarding-connection-test'));
    await settle();
    expect(screen.getByTestId('onboarding-connection-success')).toBeTruthy();

    // Continue stores the server connection the way the shell expects.
    fireEvent.click(screen.getByTestId('onboarding-next-btn'));
    await settle();
    expect(shell.config.get('connection_mode')).toBe('server');
    expect(shell.config.get('daemon_url')).toBe(ONBOARDING_URL);
    expect(shell.config.get('daemon_token')).toBe(ONBOARDING_TOKEN);

    onboarding.unmount();

    await act(async () => {
      renderSection();
      await Promise.resolve();
    });
    await settle();

    // Settings shows the same server, with the token only reported as saved.
    expect(screen.getByTestId('settings-connection-mode').textContent).toBe(
      'Connected to a Warsha server'
    );
    expect(screen.getByTestId('settings-connection-address').textContent).toBe(ONBOARDING_URL);
    expect(screen.getByTestId('settings-connection-token-state').textContent).toBe(
      'A token is saved for this server'
    );

    // Switching back to this device writes connection_mode=local.
    fireEvent.click(screen.getByTestId('settings-connection-mode-local'));
    await settle();
    fireEvent.click(screen.getByTestId('settings-connection-save'));
    await settle();

    expect(shell.config.get('connection_mode')).toBe('local');
    expect(screen.getByTestId('settings-connection-mode').textContent).toBe(
      'Warsha runs on this device'
    );
  });
});
