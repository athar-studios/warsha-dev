// apps/desktop/src/v3/pages/settings/sections/ModelsSection.test.tsx
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import {
  ALL_ROLES,
  type CustomModelRecord,
  type ModelCatalogDiff,
  type ModelInfo,
  type ProviderConfig,
  type Role,
  type RoleRouting,
  type WarshaClient,
} from '@warsha/client';
import { I18nProvider, type Locale } from '@warsha/i18n/v3';
import React from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ClientProvider } from '../../../app/ClientContext';
import { DataReadyProvider } from '../../../app/DataReadyContext';
import { DensityProvider } from '../../../app/DensityContext';
import { ThemeProvider } from '../../../app/ThemeContext';

import { ModelsSection, sanitizeSecretError } from './ModelsSection';

function makeModelInfo(id: string, name = id, provider = 'mock-provider'): ModelInfo {
  return {
    id,
    name,
    provider,
    context_window: 128000,
    max_output: 4096,
    pricing: { input_per_million_cents: 0, output_per_million_cents: 0 },
  };
}

function makeDiff(
  provider: string,
  updated: ModelInfo[],
  newModels: ModelInfo[],
  retainedCount: number
): ModelCatalogDiff {
  return {
    provider,
    updated_catalog: updated,
    new_models: newModels,
    retained_count: retainedCount,
    total_count: updated.length,
  };
}

const DEFAULT_PROVIDERS: ProviderConfig[] = [
  { name: 'mock-provider', kind: 'openai', enabled: true },
  { name: 'local-ollama', kind: 'ollama', enabled: false, api_base: 'http://localhost:11434' },
];

const DEFAULT_MODELS: CustomModelRecord[] = [
  {
    provider: 'mock-provider',
    modelId: 'gpt-4o',
    displayName: 'GPT-4o',
    contextWindow: 128000,
    enabled: true,
  },
];

const DEFAULT_ROLES: RoleRouting[] = [
  {
    role: 'intake',
    targets: [{ provider_id: 'mock-provider', model_id: 'gpt-4o', weight: 100 }],
    fallback_enabled: true,
    budget_cents_per_run: 0,
    daily_spend_cap_cents: null,
  },
];

function createClient(overrides: Partial<WarshaClient> = {}): WarshaClient {
  return {
    listProviders: vi.fn().mockResolvedValue(DEFAULT_PROVIDERS),
    listCustomModels: vi.fn().mockResolvedValue(DEFAULT_MODELS),
    listRoleRoutings: vi.fn().mockResolvedValue(DEFAULT_ROLES),
    providerAuthStatus: vi.fn().mockResolvedValue({ kind: 'none' }),
    checkProviderModels: vi.fn().mockResolvedValue(
      makeDiff(
        'mock-provider',
        [makeModelInfo('gpt-4o', 'GPT-4o'), makeModelInfo('gpt-4o-mini', 'GPT-4o Mini')],
        [makeModelInfo('gpt-4o-mini', 'GPT-4o Mini')],
        1
      )
    ),
    testProvider: vi.fn().mockResolvedValue(undefined),
    addProvider: vi.fn().mockResolvedValue(undefined),
    removeProvider: vi.fn().mockResolvedValue(undefined),
    addCustomModel: vi.fn().mockResolvedValue(undefined),
    removeCustomModel: vi.fn().mockResolvedValue(undefined),
    setRoleConfig: vi.fn().mockResolvedValue(undefined),
    providerDisconnect: vi.fn().mockResolvedValue(undefined),
    providerAuthStart: vi.fn().mockResolvedValue({ kind: 'none' }),
    ...overrides,
  } as unknown as WarshaClient;
}

function renderSection(client: WarshaClient, locale: Locale = 'en'): void {
  render(
    <I18nProvider initialLocale={locale}>
      <ThemeProvider>
        <DensityProvider>
          <ClientProvider client={client}>
            <DataReadyProvider>
              <ModelsSection />
            </DataReadyProvider>
          </ClientProvider>
        </DensityProvider>
      </ThemeProvider>
    </I18nProvider>
  );
}

describe('ModelsSection - Model Discovery & Live List Diff', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('renders provider list and discovers live models with diff badges and model chips', async () => {
    const diffResult = makeDiff(
      'mock-provider',
      [
        makeModelInfo('gpt-4o', 'GPT-4o'),
        makeModelInfo('o1-preview', 'o1 Preview'),
        makeModelInfo('o3-mini', 'o3 Mini'),
      ],
      [makeModelInfo('o1-preview', 'o1 Preview'), makeModelInfo('o3-mini', 'o3 Mini')],
      1
    );

    const client = createClient({
      checkProviderModels: vi.fn().mockResolvedValue(diffResult),
    });

    renderSection(client);

    await waitFor(() => {
      expect(screen.getByTestId('provider-row-mock-provider')).toBeDefined();
    });

    const mockRow = screen.getByTestId('provider-row-mock-provider');
    const fetchBtn = within(mockRow).getByText('Fetch models');
    fireEvent.click(fetchBtn);

    await waitFor(() => {
      expect(client.checkProviderModels).toHaveBeenCalledWith('mock-provider');
    });

    // Check that diff container rendered on row
    await waitFor(() => {
      expect(screen.getByTestId('provider-models-list-mock-provider')).toBeDefined();
    });

    const diffContainer = screen.getByTestId('provider-models-list-mock-provider');
    expect(within(diffContainer).getByText('Discovered models')).toBeDefined();
    expect(within(diffContainer).getByText('2 new')).toBeDefined();
    expect(within(diffContainer).getByText('1 retained')).toBeDefined();

    // Check individual model chips
    expect(screen.getByTestId('provider-model-item-mock-provider-gpt-4o')).toBeDefined();
    expect(screen.getByTestId('provider-model-item-mock-provider-o1-preview')).toBeDefined();
    expect(screen.getByTestId('provider-model-item-mock-provider-o3-mini')).toBeDefined();
  });

  it('surfaces error on provider row when checkProviderModels fails (unknown provider or network down)', async () => {
    const client = createClient({
      checkProviderModels: vi
        .fn()
        .mockRejectedValue(new Error('Unknown provider or endpoint connection failed: 502 Bad Gateway')),
    });

    renderSection(client);

    await waitFor(() => {
      expect(screen.getByTestId('provider-row-mock-provider')).toBeDefined();
    });

    const mockRow = screen.getByTestId('provider-row-mock-provider');
    const fetchBtn = within(mockRow).getByText('Fetch models');
    fireEvent.click(fetchBtn);

    await waitFor(() => {
      expect(screen.getByTestId('provider-fetch-error-mock-provider')).toBeDefined();
    });

    const errorContainer = screen.getByTestId('provider-fetch-error-mock-provider');
    expect(
      within(errorContainer).getByText(/Unknown provider or endpoint connection failed: 502 Bad Gateway/)
    ).toBeDefined();

    // Should NOT show diff list
    expect(screen.queryByTestId('provider-models-list-mock-provider')).toBeNull();
  });

  it('sanitizes and redacts secrets from error messages without leaking credentials', async () => {
    const secretKey = 'sk-live-super-secret-key-123456789';
    const rawError = `Failed to connect with Bearer ${secretKey}: 401 Unauthorized`;

    const sanitized = sanitizeSecretError(new Error(rawError), secretKey);
    expect(sanitized).not.toContain(secretKey);
    expect(sanitized).toContain('[redacted]');

    const client = createClient({
      checkProviderModels: vi.fn().mockRejectedValue(new Error(rawError)),
    });

    renderSection(client);

    await waitFor(() => {
      expect(screen.getByTestId('provider-row-mock-provider')).toBeDefined();
    });

    const mockRow = screen.getByTestId('provider-row-mock-provider');
    const fetchBtn = within(mockRow).getByText('Fetch models');
    fireEvent.click(fetchBtn);

    await waitFor(() => {
      expect(screen.getByTestId('provider-fetch-error-mock-provider')).toBeDefined();
    });

    const errorContainer = screen.getByTestId('provider-fetch-error-mock-provider');
    expect(errorContainer.textContent).not.toContain(secretKey);
    expect(errorContainer.textContent).toContain('[redacted]');
  });

  it('supports model discovery inside Add Provider dialog with live list and manual fallback warning', async () => {
    const dialogDiffResult = makeDiff(
      'my-ollama',
      [makeModelInfo('llama3:8b', 'Llama 3 8B'), makeModelInfo('qwen2.5-coder:7b', 'Qwen 2.5 Coder 7B')],
      [makeModelInfo('llama3:8b', 'Llama 3 8B'), makeModelInfo('qwen2.5-coder:7b', 'Qwen 2.5 Coder 7B')],
      0
    );

    const client = createClient({
      checkProviderModels: vi.fn().mockResolvedValue(dialogDiffResult),
    });

    renderSection(client);

    await waitFor(() => {
      expect(screen.getByTestId('provider-add-button')).toBeDefined();
    });

    // Open Add Provider Dialog
    fireEvent.click(screen.getByTestId('provider-add-button'));
    await waitFor(() => {
      expect(screen.getByTestId('add-provider-dialog')).toBeDefined();
    });

    // Fill form using input elements inside TextField wrappers
    const nameInput = screen.getByTestId('provider-form-name').querySelector('input')!;
    const keyInput = screen.getByTestId('provider-form-key').querySelector('input')!;
    const urlInput = screen.getByTestId('provider-form-api-base').querySelector('input')!;

    fireEvent.change(nameInput, { target: { value: 'my-ollama' } });
    fireEvent.change(keyInput, { target: { value: 'local-token-999' } });
    fireEvent.change(urlInput, { target: { value: 'http://localhost:11434' } });

    // Click Fetch Models button in dialog
    const dialogFetchBtn = screen.getByTestId('provider-form-fetch-models-btn');
    fireEvent.click(dialogFetchBtn);

    await waitFor(() => {
      expect(client.checkProviderModels).toHaveBeenCalledWith('my-ollama', {
        api_base: 'http://localhost:11434',
        key: 'local-token-999',
        kind: 'openai',
      });
    });

    // Select should now be populated
    await waitFor(() => {
      expect(screen.getByTestId('provider-form-model-select')).toBeDefined();
    });

    // Manual model fallback: typing an unoffered model triggers warning
    const manualInput = screen.getByTestId('provider-form-model-manual').querySelector('input')!;
    fireEvent.change(manualInput, { target: { value: 'nonexistent-model-xyz' } });

    await waitFor(() => {
      expect(screen.getByTestId('provider-form-model-warning')).toBeDefined();
    });
    expect(screen.getByTestId('provider-form-model-warning').textContent).toContain(
      'Model not offered by this provider'
    );

    // Typing an offered model hides warning
    fireEvent.change(manualInput, { target: { value: 'llama3:8b' } });
    await waitFor(() => {
      expect(screen.queryByTestId('provider-form-model-warning')).toBeNull();
    });
  });

  it('renders Arabic i18n labels correctly for model discovery diff and errors', async () => {
    const diffResult = makeDiff(
      'mock-provider',
      [makeModelInfo('claude-3-7-sonnet', 'Claude 3.7 Sonnet')],
      [makeModelInfo('claude-3-7-sonnet', 'Claude 3.7 Sonnet')],
      0
    );

    const client = createClient({
      checkProviderModels: vi.fn().mockResolvedValue(diffResult),
    });

    renderSection(client, 'ar');

    await waitFor(() => {
      expect(screen.getByTestId('provider-row-mock-provider')).toBeDefined();
    });

    const mockRow = screen.getByTestId('provider-row-mock-provider');
    const fetchBtn = within(mockRow).getByText('جلب النماذج');
    fireEvent.click(fetchBtn);

    await waitFor(() => {
      expect(screen.getByTestId('provider-models-list-mock-provider')).toBeDefined();
    });

    const diffContainer = screen.getByTestId('provider-models-list-mock-provider');
    expect(within(diffContainer).getByText('النماذج المكتشفة')).toBeDefined();
    expect(within(diffContainer).getByText('1 جديد')).toBeDefined();
    expect(within(diffContainer).getByText('0 باقٍ')).toBeDefined();
  });

  it('N2a proving test: renders unknown fallback label when authStatus.kind is undefined', async () => {
    const client = createClient({
      providerAuthStatus: vi.fn().mockResolvedValue({ kind: undefined } as unknown as { kind: 'none' }),
    });

    renderSection(client, 'en');

    await waitFor(() => {
      expect(screen.getByTestId('provider-row-mock-provider')).toBeDefined();
    });

    // Must NOT render literal "settings.providers.auth.undefined"
    expect(screen.queryByText('settings.providers.auth.undefined')).toBeNull();
    // Must render fallback "Unknown" under the provider card
    const mockRow = screen.getByTestId('provider-row-mock-provider');
    expect(within(mockRow).getByText('Unknown')).toBeDefined();
  });

  it('N2a proving test (Arabic): renders غير معروف when authStatus.kind is undefined', async () => {
    const client = createClient({
      providerAuthStatus: vi.fn().mockResolvedValue({ kind: undefined } as unknown as { kind: 'none' }),
    });

    renderSection(client, 'ar');

    await waitFor(() => {
      expect(screen.getByTestId('provider-row-mock-provider')).toBeDefined();
    });

    expect(screen.queryByText('settings.providers.auth.undefined')).toBeNull();
    const mockRow = screen.getByTestId('provider-row-mock-provider');
    expect(within(mockRow).getByText('غير معروف')).toBeDefined();
  });

  it('N5 proving test: add-provider dialog is constrained to viewport and closes on Escape', async () => {
    const client = createClient();
    renderSection(client, 'en');

    await waitFor(() => {
      expect(screen.getByTestId('provider-add-button')).toBeDefined();
    });

    fireEvent.click(screen.getByTestId('provider-add-button'));

    await waitFor(() => {
      expect(screen.getByTestId('add-provider-dialog')).toBeDefined();
    });

    const dialog = screen.getByTestId('add-provider-dialog');
    expect(dialog.className).toContain('v3-provider-dialog');
    expect(dialog.style.maxHeight).toContain('min(90dvh');
    expect(dialog.style.maxBlockSize).toContain('min(90dvh');

    // Press Escape key on dialog
    fireEvent.keyDown(dialog, { key: 'Escape' });

    await waitFor(() => {
      expect(screen.queryByTestId('add-provider-dialog')).toBeNull();
    });
  });

  describe('sanitizeSecretError', () => {
    it('strips duplicate HTTP prefixes and RPC envelopes', () => {
      expect(
        sanitizeSecretError('HTTP 400: HTTP 400: Bad Request')
      ).toBe('HTTP 400: Bad Request');

      expect(
        sanitizeSecretError('RPC error -32603: HTTP 500: HTTP 500: Server Failure')
      ).toBe('HTTP 500: Server Failure');
    });

    it('sanitizes raw RPC JSON dump into clean user message', () => {
      expect(
        sanitizeSecretError('HTTP 401: {"error":{"message":"Incorrect API key provided: sk-abcdef12345"}}', 'sk-abcdef12345')
      ).toBe('HTTP 401: Incorrect API key provided: [redacted]');

      expect(
        sanitizeSecretError('HTTP 400: HTTP 400: {"error":"Model not supported"}')
      ).toBe('HTTP 400: Model not supported');

      expect(
        sanitizeSecretError('{"message":"Rate limit reached"}')
      ).toBe('Rate limit reached');
    });

    it('redacts sensitive API keys and tokens', () => {
      expect(
        sanitizeSecretError('Failed with Bearer eyJhbGciOiJIUzI1NiJ9 and sk-1234567890abcdef')
      ).toBe('Failed with Bearer [redacted] and [redacted]');
    });
  });

  it('resets save button loading state on failure when adding a provider', async () => {
    const client = createClient({
      addProvider: vi.fn().mockRejectedValue(new Error('HTTP 401: HTTP 401: {"error":{"message":"Unauthorized key"}}')),
    });
    renderSection(client, 'en');

    await waitFor(() => {
      expect(screen.getByTestId('provider-add-button')).toBeDefined();
    });

    fireEvent.click(screen.getByTestId('provider-add-button'));
    await waitFor(() => {
      expect(screen.getByTestId('add-provider-dialog')).toBeDefined();
    });

    const nameInput = screen.getByTestId('provider-form-name').querySelector('input')!;
    const keyInput = screen.getByTestId('provider-form-key').querySelector('input')!;
    fireEvent.change(nameInput, { target: { value: 'test-provider' } });
    fireEvent.change(keyInput, { target: { value: 'sk-secret-key-123' } });

    const saveBtn = screen.getByRole('button', { name: 'Save provider' });
    expect(saveBtn).not.toHaveAttribute('aria-busy');

    fireEvent.click(saveBtn);

    // Error banner renders sanitized message (single prefix, extracted from JSON, secret redacted)
    await waitFor(() => {
      expect(screen.getByRole('alert')).toBeDefined();
      expect(screen.getByRole('alert')).toHaveTextContent('HTTP 401: Unauthorized key');
    });

    // Save button loading state is reset
    expect(saveBtn).not.toHaveAttribute('aria-busy');
    expect(saveBtn).not.toBeDisabled();
  });

  it('shows all 13 roles and maps stored p_m and q_a routings onto Product Manager and QA', async () => {
    const storedRoles: Array<{ role: string; modelId: string }> = [
      { role: 'intake', modelId: 'intake-model' },
      { role: 'researcher', modelId: 'researcher-model' },
      { role: 'p_m', modelId: 'pm-model' },
      { role: 'designer', modelId: 'designer-model' },
      { role: 'architect', modelId: 'architect-model' },
      { role: 'coder', modelId: 'coder-model' },
      { role: 'reviewer', modelId: 'reviewer-model' },
      { role: 'q_a', modelId: 'qa-model' },
      { role: 'security', modelId: 'security-model' },
      { role: 'release', modelId: 'release-model' },
      { role: 'marketer', modelId: 'marketer-model' },
      { role: 'feedback', modelId: 'feedback-model' },
      { role: 'secretary', modelId: 'secretary-model' },
    ];
    const routings = storedRoles.map(
      (entry): RoleRouting => ({
        role: entry.role as Role,
        targets: [{ provider_id: 'mock-provider', model_id: entry.modelId, weight: 100 }],
        fallback_enabled: entry.role === 'p_m' ? false : true,
        budget_cents_per_run: entry.role === 'p_m' ? 15 : entry.role === 'q_a' ? 9 : 0,
        daily_spend_cap_cents: entry.role === 'q_a' ? 40 : null,
      })
    );
    const client = createClient({
      listRoleRoutings: vi.fn().mockResolvedValue(routings),
    });

    renderSection(client, 'en');

    await waitFor(() => {
      expect(screen.getByTestId('role-route-select-secretary')).toBeDefined();
    });

    const labels: Record<Role, string> = {
      intake: 'Intake',
      researcher: 'Researcher',
      pm: 'Product Manager',
      designer: 'Designer',
      architect: 'Architect',
      coder: 'Coder',
      reviewer: 'Reviewer',
      qa: 'QA',
      security: 'Security',
      release: 'Release',
      marketer: 'Marketer',
      feedback: 'Feedback',
      secretary: 'Secretary',
    };
    expect(ALL_ROLES).toHaveLength(13);
    const table = screen.getByRole('table', { name: 'Who works with which model' });
    for (const role of ALL_ROLES) {
      expect(within(table).getByText(labels[role])).toBeDefined();
      expect(within(table).queryByText(`settings.role.${role}`)).toBeNull();
    }

    const pmSelect = within(screen.getByTestId('role-route-select-pm')).getByRole('combobox');
    const qaSelect = within(screen.getByTestId('role-route-select-qa')).getByRole('combobox');
    const secretarySelect = within(screen.getByTestId('role-route-select-secretary')).getByRole(
      'combobox'
    );
    expect((pmSelect as HTMLSelectElement).value).toBe('mock-provider::pm-model');
    expect((qaSelect as HTMLSelectElement).value).toBe('mock-provider::qa-model');
    expect((secretarySelect as HTMLSelectElement).value).toBe('mock-provider::secretary-model');
    expect((pmSelect as HTMLSelectElement).value).not.toBe('');
    expect(within(screen.getByTestId('role-route-select-pm')).queryByDisplayValue('No model')).toBeNull();

    const saveButtons = within(table).getAllByRole('button', { name: 'Save route' });
    const pmIndex = ALL_ROLES.indexOf('pm');
    const qaIndex = ALL_ROLES.indexOf('qa');
    fireEvent.click(saveButtons[pmIndex]!);
    fireEvent.click(saveButtons[qaIndex]!);

    await waitFor(() => {
      expect(client.setRoleConfig).toHaveBeenCalledWith({
        role: 'pm',
        targets: [{ provider_id: 'mock-provider', model_id: 'pm-model', weight: 100 }],
        fallback_enabled: false,
        budget_cents_per_run: 15,
        daily_spend_cap_cents: null,
      });
      expect(client.setRoleConfig).toHaveBeenCalledWith({
        role: 'qa',
        targets: [{ provider_id: 'mock-provider', model_id: 'qa-model', weight: 100 }],
        fallback_enabled: true,
        budget_cents_per_run: 9,
        daily_spend_cap_cents: 40,
      });
    });
  });
});
