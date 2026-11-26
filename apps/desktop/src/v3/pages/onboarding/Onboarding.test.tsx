import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import {
  ALL_ROLES,
  type ModelCatalogDiff,
  type ProjectId,
  type ProjectSummary,
  type ProviderConfig,
  type RunId,
  type RunSummary,
  type WarshaClient,
} from '@warsha/client';
import { I18nProvider } from '@warsha/i18n/v3';
import React from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CapabilitiesProvider } from '../../app/CapabilitiesContext';
import { ClientProvider } from '../../app/ClientContext';
import { DataReadyProvider, useDataReady } from '../../app/DataReadyContext';
import { clearToastListeners } from '../../app/toast';
import { RouterProvider, useRoute } from '../../router';

import OnboardingPage, {
  deriveProjectName,
  findDefaultBudget,
  isMaskedKey,
  isValidHttpUrl,
  MASKED_KEY_PLACEHOLDER,
  PROVIDER_PRESETS,
  sanitizeSecretError,
} from './index';

function RouteInspector(): React.ReactElement {
  const { pathname } = useRoute();
  return <div data-testid="route-inspector">{pathname}</div>;
}

function ReadyInspector(): React.ReactElement {
  const { isReady } = useDataReady();
  return <div data-testid="ready-inspector">{String(isReady)}</div>;
}

function makeMockClient(overrides: Partial<WarshaClient> = {}): WarshaClient {
  const createdProject: ProjectSummary = {
    id: 'proj-123' as ProjectId,
    name: 'Build store',
    createdAt: '2026-10-01T08:00:00.000Z',
    updatedAt: '2026-10-01T08:00:00.000Z',

  };
  const startedRun: RunSummary = {
    id: 'run-456' as RunId,
    projectId: 'proj-123' as ProjectId,
    stage: 'idea',
    status: 'running', runMode: 'agent',
    startedAt: '2026-10-01T08:00:00.000Z',
    updatedAt: '2026-10-01T08:00:00.000Z',

  };
  const modelDiff: ModelCatalogDiff = {
    provider: 'deep_seek',
    new_models: [],
    retained_count: 1,
    total_count: 1,
    updated_catalog: [
      {
        id: 'deepseek-chat',
        name: 'DeepSeek Chat',
        provider: 'deep_seek',
        context_window: 64000,
        max_output: 8192,
        pricing: { input_per_million_cents: 14, output_per_million_cents: 28 },
      },
    ],
  };

  return {
    health: vi.fn().mockResolvedValue({ status: 'ok', version: '0.2.1' }),
    listProviders: vi.fn().mockResolvedValue([]),
    addProvider: vi.fn().mockResolvedValue({ name: 'deep_seek', kind: 'deep_seek', enabled: true }),
    removeProvider: vi.fn().mockResolvedValue(undefined),
    testProvider: vi.fn().mockResolvedValue(undefined),
    checkProviderModels: vi.fn().mockResolvedValue(modelDiff),
    addCustomModel: vi.fn().mockResolvedValue({
      provider: 'deep_seek',
      model_id: 'deepseek-chat',
      display_name: null,
      context_window: null,
      enabled: true,
    }),
    setRoleConfig: vi.fn().mockResolvedValue(undefined),
    createProject: vi.fn().mockResolvedValue(createdProject),
    startRun: vi.fn().mockResolvedValue(startedRun),
    setProjectBudget: vi.fn().mockResolvedValue(undefined),
    listPreferences: vi.fn().mockResolvedValue([]),
    ...overrides,
  } as unknown as WarshaClient;
}

async function renderOnboarding(
  options: {
    client?: WarshaClient;
    initialPath?: string;
    locale?: 'ar' | 'en';
  } = {}
) {
  const { client = makeMockClient(), initialPath = '/onboarding', locale = 'ar' } = options;

  let utils!: ReturnType<typeof render>;
  await act(async () => {
    utils = render(
      <ClientProvider client={client}>
        <CapabilitiesProvider client={client}>
          <I18nProvider initialLocale={locale}>
            <DataReadyProvider>
              <RouterProvider initialPath={initialPath}>
                <RouteInspector />
                <ReadyInspector />
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

/**
 * The shell calls onboarding makes: config reads/writes plus the daemon connection
 * test. Tests get a stateful in-memory config map and a swappable test reply, so the
 * real `invoke` path runs instead of a stubbed module.
 */
interface ShellCall {
  cmd: string;
  args?: Record<string, unknown>;
}

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

/** Let the queued config writes and reloads settle before asserting. */
async function settle(): Promise<void> {
  await act(async () => {
    for (let i = 0; i < 12; i += 1) {
      await Promise.resolve();
    }
  });
}

/** Advance one step with the wizard's Next button. */
async function clickNext(): Promise<void> {
  fireEvent.click(screen.getByTestId('onboarding-next-btn'));
  await settle();
}

/** Go back one step with the wizard's Back button. */
async function clickBack(): Promise<void> {
  fireEvent.click(screen.getByTestId('onboarding-back-btn'));
  await settle();
}

function fieldInput(testId: string): HTMLInputElement {
  const input = screen.getByTestId(testId).querySelector('input');
  if (!input) throw new Error(`No input inside ${testId}`);
  return input as HTMLInputElement;
}

describe('Onboarding helper utilities', () => {
  it('validates HTTP/HTTPS URLs cleanly', () => {
    expect(isValidHttpUrl('https://api.example.com/v1')).toBe(true);
    expect(isValidHttpUrl('http://127.0.0.1:11434')).toBe(true);
    expect(isValidHttpUrl('ftp://example.com')).toBe(false);
    expect(isValidHttpUrl('not a url')).toBe(false);
    expect(isValidHttpUrl('')).toBe(false);
  });

  it('redacts secret keys from daemon error messages', () => {
    const err = new Error('auth failed for token sk-secret-12345 on server');
    expect(sanitizeSecretError(err, 'sk-secret-12345')).toBe(
      'auth failed for token [redacted] on server'
    );
    expect(sanitizeSecretError(err, '')).toBe(err.message);
  });

  it('derives project name from first line up to 60 chars', () => {
    expect(deriveProjectName('My Store\nWith inventory')).toBe('My Store');
    const longLine = 'A'.repeat(80);
    expect(deriveProjectName(longLine)).toHaveLength(60);
  });

  it('findDefaultBudget returns 10 by default or parses default_budget_usd preference', () => {
    expect(findDefaultBudget(null)).toBe(10);
    expect(findDefaultBudget(undefined)).toBe(10);
    expect(findDefaultBudget([])).toBe(10);
    expect(findDefaultBudget([{ key: 'other', score: 50 }])).toBe(10);
    expect(findDefaultBudget([{ key: 'default_budget_usd', score: 15 }])).toBe(15);
    expect(findDefaultBudget([{ key: 'default_budget_usd', score: -5 }])).toBe(10);
    expect(findDefaultBudget([{ key: 'default_budget_usd', score: 0 }])).toBe(10);
  });
});

describe('OnboardingPage', () => {
  beforeEach(() => {
    clearToastListeners();
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

  it('renders loading skeleton while providers are pending and sets ready=false', async () => {
    const pendingResolvers: ((val: ProviderConfig[]) => void)[] = [];
    const client = makeMockClient({
      listProviders: vi.fn(
        () =>
          new Promise<ProviderConfig[]>((resolve) => {
            pendingResolvers.push(resolve);
          })
      ),
    });

    await renderOnboarding({ client });

    expect(screen.getByTestId('onboarding-loading')).toBeTruthy();
    expect(screen.getByTestId('ready-inspector').textContent).toBe('false');

    await act(async () => {
      pendingResolvers.forEach((r) => r([]));
      await Promise.resolve();
    });

    await waitFor(() => {
      expect(screen.getByTestId('ready-inspector').textContent).toBe('true');
      expect(screen.getByTestId('onboarding-step-language')).toBeTruthy();
    });
  });

  it('renders error banner and retries when providers.list fails', async () => {
    const listProviders = vi
      .fn()
      .mockRejectedValueOnce(new Error('Connection refused'))
      .mockResolvedValue([]);
    const client = makeMockClient({ listProviders });

    window.location.hash = '#/onboarding?step=provider';
    await renderOnboarding({ client, locale: 'en' });

    await waitFor(() => {
      expect(screen.getByTestId('onboarding-error-banner')).toBeTruthy();
    });

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
      await Promise.resolve();
    });

    expect(listProviders.mock.calls.length).toBeGreaterThanOrEqual(2);
  });

  it('handles step 1 language picking and enforces max 1 primary button', async () => {
    const { container } = await renderOnboarding({ locale: 'ar' });

    const arBtn = screen.getByTestId('onboarding-lang-ar');
    const enBtn = screen.getByTestId('onboarding-lang-en');
    expect(arBtn.getAttribute('aria-pressed')).toBe('true');
    expect(enBtn.getAttribute('aria-pressed')).toBe('false');

    expect(container.querySelectorAll('[data-variant="primary"]')).toHaveLength(1);

    await act(async () => {
      fireEvent.click(enBtn);
    });

    expect(enBtn.getAttribute('aria-pressed')).toBe('true');
    expect(screen.getByRole('heading', { level: 1 }).textContent).toBe('Get started');
    expect(screen.getByRole('heading', { level: 2 }).textContent).toBe('Choose your language');

    await clickNext();
    await clickNext();

    expect(screen.getByTestId('onboarding-step-provider')).toBeTruthy();
  });

  it('renders all 7 providers, empty banner, and validates API key before save-test', async () => {
    const client = makeMockClient();
    await renderOnboarding({ client, locale: 'ar' });

    await clickNext();
    await clickNext();

    expect(screen.getByTestId('onboarding-empty-banner')).toBeTruthy();
    for (const preset of PROVIDER_PRESETS) {
      expect(screen.getByTestId(`onboarding-provider-${preset.kind}`)).toBeTruthy();
    }

    // Clicking save-test with empty key flags validation error and does not dial RPC
    await act(async () => {
      fireEvent.click(screen.getByTestId('onboarding-save-test-btn'));
    });

    expect(client.addProvider).not.toHaveBeenCalled();
    expect(screen.getByText('أدخل مفتاح API')).toBeTruthy();
  });

  it('runs add + test, wipes secret, fetches models, and routes all 12 roles', async () => {
    const client = makeMockClient();
    const { container } = await renderOnboarding({ client, locale: 'en' });

    await clickNext();
    await clickNext();

    expect(container.querySelectorAll('[data-variant="primary"]')).toHaveLength(1);

    const keyInput = screen.getByLabelText(/API key/i) as HTMLInputElement;
    await act(async () => {
      fireEvent.change(keyInput, { target: { value: 'sk-warsha-accepted' } });
    });

    await act(async () => {
      fireEvent.click(screen.getByTestId('onboarding-save-test-btn'));
      await Promise.resolve();
    });

    await waitFor(() => {
      expect(screen.getByTestId('onboarding-test-success')).toBeTruthy();
      expect((screen.getByLabelText(/API key/i) as HTMLInputElement).value).toBe(
        MASKED_KEY_PLACEHOLDER
      );
    });

    expect(client.addProvider).toHaveBeenCalledWith({
      name: 'deep_seek',
      kind: 'deep_seek',
      key: 'sk-warsha-accepted',
      api_base: 'https://api.deepseek.com/v1',
    });
    expect(client.testProvider).toHaveBeenCalledWith('deep_seek');
    expect(container.querySelectorAll('[data-variant="primary"]')).toHaveLength(1);

    // Fetch models
    await act(async () => {
      fireEvent.click(screen.getByTestId('onboarding-fetch-models-btn'));
      await Promise.resolve();
    });

    await waitFor(() => {
      expect(client.checkProviderModels).toHaveBeenCalledWith('deep_seek');
      expect(screen.getByTestId('onboarding-model-select')).toBeTruthy();
    });

    // Use model
    await act(async () => {
      fireEvent.click(screen.getByTestId('onboarding-use-model-btn'));
      await Promise.resolve();
    });

    expect(client.addCustomModel).toHaveBeenCalledWith('deep_seek', 'deepseek-chat');
    expect(client.setRoleConfig).toHaveBeenCalledTimes(ALL_ROLES.length);
  });

  it('redacts secret key if testProvider fails with echoed key', async () => {
    const client = makeMockClient({
      testProvider: vi.fn().mockRejectedValue(new Error('Bad key sk-secret-xyz refused')),
    });
    await renderOnboarding({ client, locale: 'en' });

    await clickNext();
    await clickNext();

    const keyInput = screen.getByLabelText(/API key/i) as HTMLInputElement;
    await act(async () => {
      fireEvent.change(keyInput, { target: { value: 'sk-secret-xyz' } });
    });

    await act(async () => {
      fireEvent.click(screen.getByTestId('onboarding-save-test-btn'));
      await Promise.resolve();
    });

    await waitFor(() => {
      const errorBanner = screen.getByTestId('onboarding-test-error');
      expect(errorBanner.textContent).toContain('[redacted]');
      expect(errorBanner.textContent).not.toContain('sk-secret-xyz');
    });
  });

  it('handles custom endpoint validation and 12-role routing on save-test', async () => {
    const client = makeMockClient();
    await renderOnboarding({ client, locale: 'en' });

    await clickNext();
    await clickNext();

    await act(async () => {
      fireEvent.click(screen.getByTestId('onboarding-provider-custom'));
    });

    await act(async () => {
      fireEvent.change(screen.getByLabelText(/Provider name/i), { target: { value: 'local-vllm' } });
      fireEvent.change(screen.getByLabelText(/Base URL/i), {
        target: { value: 'http://127.0.0.1:8000/v1' },
      });
      fireEvent.change(screen.getByLabelText(/Model ID/i), { target: { value: 'qwen-32b' } });
      fireEvent.change(screen.getByLabelText(/API key/i), { target: { value: 'sk-custom-token' } });
    });

    await act(async () => {
      fireEvent.click(screen.getByTestId('onboarding-save-test-btn'));
      await Promise.resolve();
    });

    await waitFor(() => {
      expect(client.addProvider).toHaveBeenCalledWith({
        name: 'local-vllm',
        kind: 'custom',
        key: 'sk-custom-token',
        api_base: 'http://127.0.0.1:8000/v1',
      });
      expect(client.addCustomModel).toHaveBeenCalledWith('local-vllm', 'qwen-32b');
      expect(client.setRoleConfig).toHaveBeenCalledTimes(ALL_ROLES.length);
      expect(client.testProvider).toHaveBeenCalledWith('local-vllm');
    });
  });

  it('handles step 3 idea creation (projects.create + runs.start + config + nav) and later skip', async () => {
    const setConfigSpy = vi.fn().mockResolvedValue(undefined);
    (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
      invoke: (cmd: string, args: unknown) => {
        if (cmd === 'set_config') return setConfigSpy(args);
        return Promise.resolve('');
      },
      transformCallback: () => 1,
      unregisterCallback: () => undefined,
      convertFileSrc: (p: string) => p,
    };

    const client = makeMockClient();
    const { container } = await renderOnboarding({ client, locale: 'en' });

    // language -> connection -> provider -> github -> idea
    await clickNext();
    await clickNext();
    await clickNext();
    await clickNext();

    expect(screen.getByTestId('onboarding-step-idea')).toBeTruthy();
    expect(container.querySelectorAll('[data-variant="primary"]')).toHaveLength(1);

    // Back returns to the GitHub step
    await clickBack();
    expect(screen.getByTestId('onboarding-step-github')).toBeTruthy();
    await clickNext();
    expect(screen.getByTestId('onboarding-step-idea')).toBeTruthy();

    // Empty idea shows validation error
    await act(async () => {
      fireEvent.click(screen.getByTestId('onboarding-start-btn'));
    });
    expect(client.createProject).not.toHaveBeenCalled();

    // Valid idea creates project and starts run
    const textarea = screen.getByLabelText(/Your first idea/i);
    await act(async () => {
      fireEvent.change(textarea, { target: { value: 'Build store\nwith full cart' } });
    });

    await act(async () => {
      fireEvent.click(screen.getByTestId('onboarding-start-btn'));
      await Promise.resolve();
    });

    expect(client.createProject).toHaveBeenCalledWith(
      'Build store',
      'Build store\nwith full cart',
      'en'
    );
    expect(client.startRun).toHaveBeenCalledWith('proj-123', 'Build store\nwith full cart');
    expect(client.setProjectBudget).toHaveBeenCalledWith('proj-123', 1000);
    expect(setConfigSpy).toHaveBeenCalledWith({ key: 'onboarding_complete', value: 'true' });
    expect(screen.getByTestId('route-inspector').textContent).toBe('/p/proj-123');
  });

  it('later button sets onboarding_complete and navigates to /', async () => {
    const setConfigSpy = vi.fn().mockResolvedValue(undefined);
    (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
      invoke: (cmd: string, args: unknown) => {
        if (cmd === 'set_config') return setConfigSpy(args);
        return Promise.resolve('');
      },
      transformCallback: () => 1,
      unregisterCallback: () => undefined,
      convertFileSrc: (p: string) => p,
    };

    window.location.hash = '#/onboarding?step=idea';
    await renderOnboarding({ locale: 'en' });

    await act(async () => {
      fireEvent.click(screen.getByTestId('onboarding-later-btn'));
      await Promise.resolve();
    });

    expect(setConfigSpy).toHaveBeenCalledWith({ key: 'onboarding_complete', value: 'true' });
    expect(screen.getByTestId('route-inspector').textContent).toBe('/');
  });

  it('F1 proving test: save-only-after-success (rolls back provider if testProvider fails)', async () => {
    const client = makeMockClient({
      testProvider: vi.fn().mockRejectedValue(new Error('Connection timed out after 8s')),
    });
    await renderOnboarding({ client, locale: 'en' });

    await clickNext();
    await clickNext();

    const keyInput = screen.getByLabelText(/API key/i) as HTMLInputElement;
    await act(async () => {
      fireEvent.change(keyInput, { target: { value: 'sk-fail-key' } });
    });

    await act(async () => {
      fireEvent.click(screen.getByTestId('onboarding-save-test-btn'));
      await Promise.resolve();
    });

    await waitFor(() => {
      expect(screen.getByTestId('onboarding-test-error')).toBeTruthy();
    });

    expect(client.addProvider).toHaveBeenCalledWith({
      name: 'deep_seek',
      kind: 'deep_seek',
      key: 'sk-fail-key',
      api_base: 'https://api.deepseek.com/v1',
    });
    // Rolled back so provider is not saved
    expect(client.removeProvider).toHaveBeenCalledWith('deep_seek');
    expect(screen.queryByTestId('onboarding-test-success')).toBeNull();
  });

  it('F1 proving test: key-preserved-on-failure (retains user input on test failure)', async () => {
    const client = makeMockClient({
      testProvider: vi.fn().mockRejectedValue(new Error('Unauthorized 401')),
    });
    await renderOnboarding({ client, locale: 'en' });

    await clickNext();
    await clickNext();

    const keyInput = screen.getByLabelText(/API key/i) as HTMLInputElement;
    await act(async () => {
      fireEvent.change(keyInput, { target: { value: 'sk-preserve-me-secret-key' } });
    });

    await act(async () => {
      fireEvent.click(screen.getByTestId('onboarding-save-test-btn'));
      await Promise.resolve();
    });

    await waitFor(() => {
      expect(screen.getByTestId('onboarding-test-error')).toBeTruthy();
    });

    // Key must be preserved on failure, NOT cleared
    expect(keyInput.value).toBe('sk-preserve-me-secret-key');
  });

  it('G4 proving test: masked-key restored wizard healthy (reflects healthy state, skips re-test on save, never exposes real key)', async () => {
    const existingProvider: ProviderConfig = {
      name: 'deep_seek',
      kind: 'deep_seek',
      enabled: true,
    };
    const client = makeMockClient({
      listProviders: vi.fn().mockResolvedValue([existingProvider]),
    });

    // Relaunch / restored onboarding wizard
    window.location.hash = '#/onboarding?step=provider';
    await renderOnboarding({ client, locale: 'en', initialPath: '/onboarding?step=provider' });

    await waitFor(() => {
      const keyInput = screen.getByLabelText(/API key/i) as HTMLInputElement;
      expect(keyInput.value).toBe(MASKED_KEY_PLACEHOLDER);
    });

    // Healthy state reflected: no red failure banner, success indicator shown
    expect(screen.queryByTestId('onboarding-test-error')).toBeNull();
    expect(screen.getByTestId('onboarding-test-success')).toBeTruthy();

    const keyInput = screen.getByLabelText(/API key/i) as HTMLInputElement;
    expect(isMaskedKey(keyInput.value)).toBe(true);
    // Never displays real secret key
    expect(keyInput.value).not.toContain('sk-');

    // Submitting with masked key skips destructive re-test or re-save
    await act(async () => {
      fireEvent.click(screen.getByTestId('onboarding-save-test-btn'));
      await Promise.resolve();
    });

    // Did not invoke addProvider with placeholder
    expect(client.addProvider).not.toHaveBeenCalled();
    // Did not invoke testProvider
    expect(client.testProvider).not.toHaveBeenCalled();
    // Still healthy
    expect(screen.queryByTestId('onboarding-test-error')).toBeNull();
    expect(screen.getByTestId('onboarding-test-success')).toBeTruthy();
  });

  it('B1 proving test: Next advances after successful provider save-test with masked key', async () => {
    const client = makeMockClient();
    await renderOnboarding({ client, locale: 'en' });

    // Step 1 (language) -> Step 2 (provider)
    await clickNext();
    await clickNext();
    expect(screen.getByTestId('onboarding-step-provider')).toBeTruthy();

    // Enter real API key and verify required attribute
    const keyInput = screen.getByLabelText(/API key/i) as HTMLInputElement;
    expect(keyInput.required).toBe(true);
    await act(async () => {
      fireEvent.change(keyInput, { target: { value: 'sk-warsha-accepted' } });
    });

    // Run save & test
    await act(async () => {
      fireEvent.click(screen.getByTestId('onboarding-save-test-btn'));
      await Promise.resolve();
    });

    // Save and test succeeds, real secret key cleared from state and replaced by MASKED_KEY_PLACEHOLDER
    await waitFor(() => {
      expect(screen.getByTestId('onboarding-test-success')).toBeTruthy();
      expect((screen.getByLabelText(/API key/i) as HTMLInputElement).value).toBe(
        MASKED_KEY_PLACEHOLDER
      );
    });
    const keyInputAfterSave = screen.getByLabelText(/API key/i) as HTMLInputElement;
    expect(isMaskedKey(keyInputAfterSave.value)).toBe(true);
    expect(keyInputAfterSave.value).not.toContain('sk-');

    // Next button becomes primary and advances to the GitHub step, then the idea step
    const nextBtn = screen.getByTestId('onboarding-next-btn');
    expect(nextBtn.getAttribute('data-variant')).toBe('primary');

    await clickNext();
    expect(screen.getByTestId('onboarding-step-github')).toBeTruthy();
    await clickNext();

    // Successfully advanced to Step 5 (idea)
    expect(screen.getByTestId('onboarding-step-idea')).toBeTruthy();

    // Navigate back to the provider step through the GitHub step
    await clickBack();
    expect(screen.getByTestId('onboarding-step-github')).toBeTruthy();
    await clickBack();
    expect(screen.getByTestId('onboarding-step-provider')).toBeTruthy();

    // Provider key remains masked
    const keyInputBack = screen.getByLabelText(/API key/i) as HTMLInputElement;
    expect(keyInputBack.value).toBe(MASKED_KEY_PLACEHOLDER);

    // Subsequent save-test with mask skips re-test and does NOT dial daemon with placeholder
    vi.mocked(client.addProvider).mockClear();
    vi.mocked(client.testProvider).mockClear();
    await act(async () => {
      fireEvent.click(screen.getByTestId('onboarding-save-test-btn'));
      await Promise.resolve();
    });

    expect(client.addProvider).not.toHaveBeenCalled();
    expect(client.testProvider).not.toHaveBeenCalled();
    expect(screen.queryByTestId('onboarding-test-error')).toBeNull();

    // Next advances again to the idea step through GitHub
    await clickNext();
    await clickNext();
    expect(screen.getByTestId('onboarding-step-idea')).toBeTruthy();
  });

  it('N2 proving test: wizard start calls createProject -> startRun -> setProjectBudget in order with 1000 cents default and navigates to /p/:id', async () => {
    const callOrder: string[] = [];
    const client = makeMockClient({
      createProject: vi.fn().mockImplementation(async () => {
        callOrder.push('createProject');
        return {
          id: 'proj-n2' as ProjectId,
          name: 'N2 Project',
          description: 'Idea for N2',
          path: '/path/to/n2',
          createdAt: '2026-11-21T00:00:00.000Z',
          updatedAt: '2026-11-21T00:00:00.000Z',
        };
      }),
      startRun: vi.fn().mockImplementation(async () => {
        callOrder.push('startRun');
        return {
          id: 'run-n2' as RunId,
          projectId: 'proj-n2' as ProjectId,
          stage: 'idea',
          status: 'running',
          runMode: 'agent',
          startedAt: '2026-11-21T00:00:00.000Z',
          updatedAt: '2026-11-21T00:00:00.000Z',
        };
      }),
      setProjectBudget: vi.fn().mockImplementation(async () => {
        callOrder.push('setProjectBudget');
      }),
    });

    const setConfigSpy = vi.fn().mockResolvedValue(undefined);
    (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
      invoke: (cmd: string, args: unknown) => {
        if (cmd === 'set_config') {
          callOrder.push('markOnboardingComplete');
          return setConfigSpy(args);
        }
        return Promise.resolve('');
      },
      transformCallback: () => 1,
      unregisterCallback: () => undefined,
      convertFileSrc: (p: string) => p,
    };

    window.location.hash = '#/onboarding?step=idea';
    await renderOnboarding({ client, locale: 'en', initialPath: '/onboarding?step=idea' });

    const textarea = screen.getByLabelText(/Your first idea/i);
    await act(async () => {
      fireEvent.change(textarea, { target: { value: 'N2 Project Idea\nWith multiple lines' } });
    });

    await act(async () => {
      fireEvent.click(screen.getByTestId('onboarding-start-btn'));
      await Promise.resolve();
    });

    // Check parameters
    expect(client.createProject).toHaveBeenCalledWith(
      'N2 Project Idea',
      'N2 Project Idea\nWith multiple lines',
      'en'
    );
    expect(client.startRun).toHaveBeenCalledWith('proj-n2', 'N2 Project Idea\nWith multiple lines');
    expect(client.setProjectBudget).toHaveBeenCalledWith('proj-n2', 1000);

    // Assert exact order: createProject -> startRun -> setProjectBudget -> markOnboardingComplete
    expect(callOrder).toEqual([
      'createProject',
      'startRun',
      'setProjectBudget',
      'markOnboardingComplete',
    ]);

    // Navigated to /p/:id
    expect(screen.getByTestId('route-inspector').textContent).toBe('/p/proj-n2');
  });

  it('N2 proving test: wizard start respects custom default_budget_usd preference', async () => {
    const client = makeMockClient({
      listPreferences: vi.fn().mockResolvedValue([
        { key: 'default_budget_usd', score: 25 },
      ]),
    });

    window.location.hash = '#/onboarding?step=idea';
    await renderOnboarding({ client, locale: 'en', initialPath: '/onboarding?step=idea' });

    const textarea = screen.getByLabelText(/Your first idea/i);
    await act(async () => {
      fireEvent.change(textarea, { target: { value: 'Custom budget app' } });
    });

    await act(async () => {
      fireEvent.click(screen.getByTestId('onboarding-start-btn'));
      await Promise.resolve();
    });

    // 25 USD -> 2500 cents
    expect(client.setProjectBudget).toHaveBeenCalledWith('proj-123', 2500);
    expect(screen.getByTestId('route-inspector').textContent).toBe('/p/proj-123');
  });

  it('N2 proving test: wizard start handles setProjectBudget failure gracefully without blocking navigation or orphaning run', async () => {
    const consoleWarnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const budgetError = new Error('Budget service temporarily unreachable');
    const client = makeMockClient({
      setProjectBudget: vi.fn().mockRejectedValue(budgetError),
    });

    const setConfigSpy = vi.fn().mockResolvedValue(undefined);
    (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
      invoke: (cmd: string, args: unknown) => {
        if (cmd === 'set_config') return setConfigSpy(args);
        return Promise.resolve('');
      },
      transformCallback: () => 1,
      unregisterCallback: () => undefined,
      convertFileSrc: (p: string) => p,
    };

    window.location.hash = '#/onboarding?step=idea';
    await renderOnboarding({ client, locale: 'en', initialPath: '/onboarding?step=idea' });

    const textarea = screen.getByLabelText(/Your first idea/i);
    await act(async () => {
      fireEvent.change(textarea, { target: { value: 'Resilient project idea' } });
    });

    await act(async () => {
      fireEvent.click(screen.getByTestId('onboarding-start-btn'));
      await Promise.resolve();
    });

    // Both project and run were created
    expect(client.createProject).toHaveBeenCalled();
    expect(client.startRun).toHaveBeenCalled();
    expect(client.setProjectBudget).toHaveBeenCalledWith('proj-123', 1000);

    // Warning was logged
    expect(consoleWarnSpy).toHaveBeenCalledWith(
      'Failed to set initial project budget:',
      budgetError
    );

    // Navigation and onboarding completion were NOT blocked
    expect(setConfigSpy).toHaveBeenCalledWith({ key: 'onboarding_complete', value: 'true' });
    expect(screen.getByTestId('route-inspector').textContent).toBe('/p/proj-123');

    consoleWarnSpy.mockRestore();
  });

  it('connection step: a failed server test keeps Continue disabled and explains the reason', async () => {
    await renderOnboarding({ client: makeMockClient(), locale: 'en' });
    await clickNext();

    expect(screen.getByTestId('onboarding-step-connection')).toBeTruthy();
    expect(
      screen.getByTestId('onboarding-connection-mode-local').getAttribute('aria-checked')
    ).toBe('true');

    fireEvent.click(screen.getByTestId('onboarding-connection-mode-server'));
    await settle();
    expect(
      screen.getByTestId('onboarding-connection-mode-server').getAttribute('aria-checked')
    ).toBe('true');

    fireEvent.change(fieldInput('onboarding-connection-url'), {
      target: { value: 'http://10.0.0.5:7878' },
    });
    fireEvent.change(fieldInput('onboarding-connection-token'), {
      target: { value: 'tok-server' },
    });
    await settle();

    // A server needs a passing test for the values on screen before Continue works.
    expect((screen.getByTestId('onboarding-next-btn') as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByTestId('onboarding-connection-test-required')).toBeTruthy();

    shell.connectionReply = { ok: false, reason: 'unreachable', detail: 'dns failure', wsUrl: '' };
    fireEvent.click(screen.getByTestId('onboarding-connection-test'));
    await settle();
    expect(screen.getByTestId('onboarding-connection-error')).toBeTruthy();
    expect(screen.getByTestId('onboarding-connection-error-reason').textContent).toContain(
      'The server did not answer'
    );
    expect((screen.getByTestId('onboarding-next-btn') as HTMLButtonElement).disabled).toBe(true);
    expect(shell.config.get('connection_mode')).toBeUndefined();

    // A rejected token gets its own plain sentence, not the unreachable one.
    shell.connectionReply = { ok: false, reason: 'unauthorized', detail: 'token rejected', wsUrl: '' };
    fireEvent.click(screen.getByTestId('onboarding-connection-test'));
    await settle();
    expect(screen.getByTestId('onboarding-connection-error-reason').textContent).toContain(
      'rejected the token'
    );
    expect((screen.getByTestId('onboarding-next-btn') as HTMLButtonElement).disabled).toBe(true);

    // A rejected token never leaks back into the explanation.
    expect(screen.getByTestId('onboarding-connection-error').textContent).not.toContain(
      'tok-server'
    );

    // "Use this device instead" returns to Local, which keeps working.
    fireEvent.click(screen.getByTestId('onboarding-connection-use-local'));
    await settle();
    expect(
      screen.getByTestId('onboarding-connection-mode-local').getAttribute('aria-checked')
    ).toBe('true');
    expect((screen.getByTestId('onboarding-next-btn') as HTMLButtonElement).disabled).toBe(false);
    expect(screen.queryByTestId('onboarding-connection-error')).toBeNull();
  });

  it('connection step: a passing test enables Continue, edits invalidate it, and saving writes the server config', async () => {
    await renderOnboarding({ client: makeMockClient(), locale: 'en' });
    await clickNext();

    fireEvent.click(screen.getByTestId('onboarding-connection-mode-server'));
    await settle();
    fireEvent.change(fieldInput('onboarding-connection-url'), {
      target: { value: 'http://warsha.example:7878' },
    });
    fireEvent.change(fieldInput('onboarding-connection-token'), {
      target: { value: 'tok-server' },
    });
    await settle();

    shell.connectionReply = {
      ok: true,
      detail: 'ok',
      wsUrl: 'ws://warsha.example:7878/ws',
      version: '0.9.3',
    };
    fireEvent.click(screen.getByTestId('onboarding-connection-test'));
    await settle();
    expect(screen.getByTestId('onboarding-connection-success').textContent).toContain('0.9.3');
    expect((screen.getByTestId('onboarding-next-btn') as HTMLButtonElement).disabled).toBe(false);

    // Editing the address invalidates the earlier result.
    fireEvent.change(fieldInput('onboarding-connection-url'), {
      target: { value: 'http://warsha.example:9999' },
    });
    await settle();
    expect(screen.queryByTestId('onboarding-connection-success')).toBeNull();
    expect((screen.getByTestId('onboarding-next-btn') as HTMLButtonElement).disabled).toBe(true);

    // Retest the edited address, then Continue stores every key and advances.
    shell.connectionReply = {
      ok: true,
      detail: 'ok',
      wsUrl: 'ws://warsha.example:9999/ws',
      version: '0.9.3',
    };
    fireEvent.click(screen.getByTestId('onboarding-connection-test'));
    await settle();
    await clickNext();

    expect(shell.config.get('daemon_url')).toBe('http://warsha.example:9999');
    expect(shell.config.get('daemon_ws_url')).toBe('ws://warsha.example:9999/ws');
    expect(shell.config.get('daemon_token')).toBe('tok-server');
    expect(shell.config.get('connection_mode')).toBe('server');
    const writtenKeys = shell.calls
      .filter((call) => call.cmd === 'set_config')
      .map((call) => call.args?.key);
    expect(writtenKeys.slice(-4)).toEqual([
      'daemon_url',
      'daemon_ws_url',
      'daemon_token',
      'connection_mode',
    ]);
    expect(screen.getByTestId('onboarding-step-provider')).toBeTruthy();
  });

  it('connection step: choosing this device writes connection_mode=local', async () => {
    await renderOnboarding({ client: makeMockClient(), locale: 'en' });
    await clickNext();

    await clickNext();
    expect(shell.config.get('connection_mode')).toBe('local');
    expect(screen.getByTestId('onboarding-step-provider')).toBeTruthy();
  });

  it('github step: reports the connected login, shows a plain failure, and skips without blocking', async () => {
    const client = makeMockClient({
      connectIntegration: vi.fn().mockResolvedValue({ id: 'github', connected: true }),
      testIntegration: vi
        .fn()
        .mockResolvedValue({ status: 'ok', message: 'authenticated', account: 'octo-mock' }),
    });
    await renderOnboarding({ client, locale: 'en' });
    await clickNext();
    await clickNext();
    await clickNext();
    expect(screen.getByTestId('onboarding-step-github')).toBeTruthy();

    fireEvent.change(fieldInput('onboarding-github-token'), {
      target: { value: 'ghp_warsha_mock' },
    });
    fireEvent.click(screen.getByTestId('onboarding-github-connect-btn'));
    await settle();

    expect(client.connectIntegration).toHaveBeenCalledWith('github', {
      token: 'ghp_warsha_mock',
    });
    expect(screen.getByTestId('onboarding-github-connected').textContent).toContain('@octo-mock');
    // A working token is never shown back to the user.
    expect(fieldInput('onboarding-github-token').value).toBe('');

    // A rejected token shows GitHub's message plainly and never blocks the wizard.
    vi.mocked(client.testIntegration).mockResolvedValue({
      status: 'failed',
      message: 'Bad credentials',
    });
    fireEvent.change(fieldInput('onboarding-github-token'), {
      target: { value: 'ghp_wrong' },
    });
    fireEvent.click(screen.getByTestId('onboarding-github-connect-btn'));
    await settle();
    expect(screen.getByTestId('onboarding-github-error').textContent).toContain(
      'Bad credentials'
    );
    expect((screen.getByTestId('onboarding-next-btn') as HTMLButtonElement).disabled).toBe(false);

    // Skip continues to the idea step.
    fireEvent.click(screen.getByTestId('onboarding-github-skip-btn'));
    await settle();
    expect(screen.getByTestId('onboarding-step-idea')).toBeTruthy();
  });

  it('provider skip warns on the provider step and again on the idea step', async () => {
    await renderOnboarding({ client: makeMockClient(), locale: 'en' });
    await clickNext();
    await clickNext();
    expect(screen.getByTestId('onboarding-step-provider')).toBeTruthy();
    expect(screen.queryByTestId('onboarding-provider-warning')).toBeNull();

    fireEvent.click(screen.getByTestId('onboarding-provider-skip-btn'));
    await settle();
    expect(screen.getByTestId('onboarding-provider-warning').textContent).toContain(
      "Runs won't start until you connect a model provider. You can do it later from Settings → Models."
    );

    await clickNext();
    await clickNext();
    expect(screen.getByTestId('onboarding-step-idea')).toBeTruthy();
    expect(screen.getByTestId('onboarding-provider-warning').textContent).toContain(
      "Runs won't start until you connect a model provider"
    );
  });

  it('renders the connection and GitHub steps in Arabic with a right-to-left document', async () => {
    await renderOnboarding({ client: makeMockClient(), locale: 'ar' });
    await clickNext();

    expect(screen.getByTestId('onboarding-step-connection')).toBeTruthy();
    expect(document.documentElement.getAttribute('dir')).toBe('rtl');
    expect(screen.getByTestId('onboarding-step-connection').textContent).toContain(
      'كيف نُشغّل ورشة؟'
    );

    fireEvent.click(screen.getByTestId('onboarding-connection-mode-server'));
    await settle();
    expect(screen.getByTestId('onboarding-connection-mode-server').textContent).toContain('خادم');
    // Technical fields stay left-to-right even inside an RTL page.
    expect(fieldInput('onboarding-connection-url').getAttribute('dir')).toBe('ltr');

    // Back to this device so the wizard can move on without a server test.
    fireEvent.click(screen.getByTestId('onboarding-connection-mode-local'));
    await settle();
    await clickNext();
    await clickNext();
    expect(screen.getByTestId('onboarding-step-github')).toBeTruthy();
    expect(screen.getByTestId('onboarding-step-github').textContent).toContain('GitHub');
    expect(screen.getByTestId('onboarding-github-token').textContent).toContain('رمز GitHub');
    expect(screen.getByTestId('onboarding-github-skip-btn').textContent).toBe('تخطٍّ الآن');
  });
});
