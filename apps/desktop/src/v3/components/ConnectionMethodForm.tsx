/* apps/desktop/src/v3/components/ConnectionMethodForm.tsx — shared by onboarding and Settings → Connection.
 * One component, one behaviour: pick this device or a server, prove the server answers before it can be saved.
 * The config contract is the shell's (invoke get_config / set_config with connection_mode, daemon_url,
 * daemon_ws_url, daemon_token) and test_daemon_connection; nothing here reaches into crates/ or packages/client. */
import { invoke } from '@tauri-apps/api/core';
import { useT } from '@warsha/i18n/v3';
import { Banner, Button, IconCheck, IconRefresh, TextField } from '@warsha/ui/v3';
import React, { useCallback, useState } from 'react';

import './ConnectionMethodForm.css';

export type ConnectionMode = 'local' | 'server';

export type ConnectionFailureReason = 'invalid_url' | 'unreachable' | 'unauthorized' | 'bad_response';

/** The shell's reply from `test_daemon_connection`. */
export interface ConnectionTestResult {
  ok: boolean;
  reason?: ConnectionFailureReason;
  detail: string;
  wsUrl: string;
  version?: string;
}

/** What the daemon currently has stored, read through get_config. The token value is never kept. */
export interface ConnectionStoredState {
  mode: ConnectionMode;
  url: string;
  tokenSet: boolean;
}

export interface ConnectionMethodDraft {
  mode: ConnectionMode;
  url: string;
  token: string;
}

/** The browser build has no shell, so the same check the folder picker uses. */
export function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export function isValidServerUrl(value: string): boolean {
  return /^https?:\/\/[^\s/]+/i.test(value.trim());
}

const REASON_KEYS: Record<ConnectionFailureReason, string> = {
  invalid_url: 'onboarding.connection_error_invalid_url',
  unreachable: 'onboarding.connection_error_unreachable',
  unauthorized: 'onboarding.connection_error_unauthorized',
  bad_response: 'onboarding.connection_error_bad_response',
};

/** The plain-language explanation key for one failure reason. */
export function connectionFailureKey(reason?: string): string {
  if (reason === 'invalid_url' || reason === 'unreachable' || reason === 'unauthorized' || reason === 'bad_response') {
    return REASON_KEYS[reason];
  }
  return 'onboarding.connection_error_generic';
}

/** A token must never come back to the screen, not even inside a server's error text. */
export function redactSecret(text: string, secret: string): string {
  const trimmedSecret = secret.trim();
  if (!trimmedSecret) return text;
  return text.split(trimmedSecret).join('[redacted]');
}

/**
 * Ask the shell to dial the server. A malformed address is rejected here so the user gets the
 * same explanation offline; anything the shell throws is reported as unreachable.
 */
export async function testServerConnection(
  url: string,
  token: string
): Promise<ConnectionTestResult> {
  const cleanUrl = url.trim();
  if (!isValidServerUrl(cleanUrl)) {
    return { ok: false, reason: 'invalid_url', detail: '', wsUrl: '' };
  }
  if (!isTauriRuntime()) {
    return { ok: false, reason: 'unreachable', detail: '', wsUrl: '' };
  }
  try {
    const reply = await invoke<ConnectionTestResult>('test_daemon_connection', {
      url: cleanUrl,
      token: token.trim(),
    });
    if (!reply || typeof reply.ok !== 'boolean') {
      return { ok: false, reason: 'bad_response', detail: '', wsUrl: '' };
    }
    return {
      ok: reply.ok,
      ...(reply.reason ? { reason: reply.reason } : {}),
      detail: typeof reply.detail === 'string' ? reply.detail : '',
      wsUrl: typeof reply.wsUrl === 'string' ? reply.wsUrl : '',
      ...(typeof reply.version === 'string' && reply.version ? { version: reply.version } : {}),
    };
  } catch (err) {
    return {
      ok: false,
      reason: 'unreachable',
      detail: err instanceof Error ? err.message : '',
      wsUrl: '',
    };
  }
}

/**
 * Write the choice. Server writes the endpoint, the event URL and the token first, then flips
 * the mode last, so a half-written endpoint never becomes the active connection.
 */
export async function saveConnection(choice: {
  mode: ConnectionMode;
  url: string;
  token: string;
  wsUrl?: string;
}): Promise<void> {
  if (choice.mode === 'local') {
    await invoke('set_config', { key: 'connection_mode', value: 'local' });
    return;
  }
  await invoke('set_config', { key: 'daemon_url', value: choice.url.trim() });
  await invoke('set_config', { key: 'daemon_ws_url', value: (choice.wsUrl ?? '').trim() });
  await invoke('set_config', { key: 'daemon_token', value: choice.token.trim() });
  await invoke('set_config', { key: 'connection_mode', value: 'server' });
}

/** Read the stored connection. Outside the desktop shell there is nothing to read: this device. */
export async function readConnectionConfig(): Promise<ConnectionStoredState> {
  if (!isTauriRuntime()) return { mode: 'local', url: '', tokenSet: false };
  try {
    const mode = await invoke<string>('get_config', { key: 'connection_mode' });
    const url = await invoke<string>('get_config', { key: 'daemon_url' });
    const token = await invoke<string>('get_config', { key: 'daemon_token' });
    return {
      mode: mode === 'server' ? 'server' : 'local',
      url: typeof url === 'string' ? url : '',
      tokenSet: typeof token === 'string' && token.trim().length > 0,
    };
  } catch {
    return { mode: 'local', url: '', tokenSet: false };
  }
}

export interface UseConnectionMethodResult {
  draft: ConnectionMethodDraft;
  result: ConnectionTestResult | null;
  testing: boolean;
  /** Continue may proceed: local always; server only once this exact endpoint+token passed. */
  isVerified: boolean;
  setMode: (mode: ConnectionMode) => void;
  setUrl: (url: string) => void;
  setToken: (token: string) => void;
  test: () => Promise<ConnectionTestResult>;
  useLocal: () => void;
}

/**
 * Editing either field after a passed test clears the result, so a stale pass can never be saved
 * against values that were never tested.
 */
export function useConnectionMethod(
  initial: Partial<ConnectionMethodDraft> = {}
): UseConnectionMethodResult {
  const [draft, setDraft] = useState<ConnectionMethodDraft>({
    mode: initial.mode ?? 'local',
    url: initial.url ?? '',
    token: initial.token ?? '',
  });
  const [result, setResult] = useState<ConnectionTestResult | null>(null);
  const [testing, setTesting] = useState<boolean>(false);

  const setMode = useCallback((mode: ConnectionMode): void => {
    setDraft((prev) => ({ ...prev, mode }));
    setResult(null);
  }, []);

  const setUrl = useCallback((url: string): void => {
    setDraft((prev) => ({ ...prev, url }));
    setResult(null);
  }, []);

  const setToken = useCallback((token: string): void => {
    setDraft((prev) => ({ ...prev, token }));
    setResult(null);
  }, []);

  const useLocal = useCallback((): void => {
    setDraft((prev) => ({ ...prev, mode: 'local' }));
    setResult(null);
  }, []);

  const test = useCallback(async (): Promise<ConnectionTestResult> => {
    setTesting(true);
    try {
      const outcome = await testServerConnection(draft.url, draft.token);
      setResult(outcome);
      return outcome;
    } finally {
      setTesting(false);
    }
  }, [draft.url, draft.token]);

  const isVerified = draft.mode === 'local' || result?.ok === true;

  return { draft, result, testing, isVerified, setMode, setUrl, setToken, test, useLocal };
}

export interface ConnectionMethodFormProps {
  draft: ConnectionMethodDraft;
  result: ConnectionTestResult | null;
  testing: boolean;
  /** Prefix for data-testid: `onboarding` or `settings`. */
  testIdPrefix: string;
  disabled?: boolean;
  onModeChange: (mode: ConnectionMode) => void;
  onUrlChange: (url: string) => void;
  onTokenChange: (token: string) => void;
  onTest: () => void;
  onUseLocal: () => void;
  footer?: React.ReactNode;
}

export function ConnectionMethodForm({
  draft,
  result,
  testing,
  testIdPrefix,
  disabled = false,
  onModeChange,
  onUrlChange,
  onTokenChange,
  onTest,
  onUseLocal,
  footer,
}: ConnectionMethodFormProps): React.ReactElement {
  const t = useT();
  const inShell = isTauriRuntime();
  const isServer = draft.mode === 'server';
  const failed = result !== null && !result.ok;

  return (
    <div className="v3-connection-form" data-testid={`${testIdPrefix}-connection-form`}>
      <div
        className="v3-connection-modes"
        role="radiogroup"
        aria-label={t('onboarding.connection_mode_label')}
      >
        <button
          type="button"
          role="radio"
          aria-checked={draft.mode === 'local'}
          className="v3-connection-mode"
          data-action="connection.mode.pick"
          data-mode="local"
          data-testid={`${testIdPrefix}-connection-mode-local`}
          disabled={disabled}
          onClick={() => onModeChange('local')}
        >
          <span className="v3-connection-mode-title">{t('onboarding.connection_mode_local')}</span>
          <span className="v3-connection-mode-desc">
            {t('onboarding.connection_mode_local_desc')}
          </span>
        </button>

        <button
          type="button"
          role="radio"
          aria-checked={isServer}
          className="v3-connection-mode"
          data-action="connection.mode.pick"
          data-mode="server"
          data-testid={`${testIdPrefix}-connection-mode-server`}
          disabled={disabled}
          onClick={() => onModeChange('server')}
        >
          <span className="v3-connection-mode-title">{t('onboarding.connection_mode_server')}</span>
          <span className="v3-connection-mode-desc">
            {t('onboarding.connection_mode_server_desc')}
          </span>
        </button>
      </div>

      {isServer && (
        <div className="v3-connection-server">
          {!inShell && (
            <Banner variant="info" data-testid={`${testIdPrefix}-connection-browser-notice`}>
              {t('onboarding.connection_browser_notice')}
            </Banner>
          )}

          <TextField
            label={t('onboarding.connection_url')}
            placeholder={t('onboarding.connection_url_placeholder')}
            hint={t('onboarding.connection_url_hint')}
            value={draft.url}
            onChange={(e) => onUrlChange(e.target.value)}
            dir="ltr"
            autoComplete="off"
            data-testid={`${testIdPrefix}-connection-url`}
          />

          <TextField
            type="password"
            label={t('onboarding.connection_token')}
            placeholder={t('onboarding.connection_token_placeholder')}
            hint={t('onboarding.connection_token_hint')}
            value={draft.token}
            onChange={(e) => onTokenChange(e.target.value)}
            dir="ltr"
            autoComplete="off"
            data-testid={`${testIdPrefix}-connection-token`}
          />

          <div className="v3-connection-test-row">
            <Button
              variant="secondary"
              icon={<IconRefresh size={16} />}
              loading={testing}
              disabled={!inShell || disabled}
              data-action="connection.test"
              data-testid={`${testIdPrefix}-connection-test`}
              onClick={onTest}
            >
              {testing ? t('onboarding.connection_testing') : t('onboarding.connection_test')}
            </Button>
            {!inShell && (
              <span className="v3-connection-note">{t('onboarding.connection_browser_test')}</span>
            )}
          </div>

          {result?.ok && (
            <Banner
              variant="success"
              icon={<IconCheck size={20} />}
              data-testid={`${testIdPrefix}-connection-success`}
            >
              {result.version
                ? t('onboarding.connection_success_version', { version: result.version })
                : t('onboarding.connection_success')}
            </Banner>
          )}

          {failed && (
            <Banner
              variant="danger"
              title={t('onboarding.connection_error_title')}
              data-testid={`${testIdPrefix}-connection-error`}
            >
              <span data-testid={`${testIdPrefix}-connection-error-reason`}>
                {t(connectionFailureKey(result?.reason))}
              </span>
              {result?.detail ? (
                <span className="v3-connection-detail">
                  <bdi dir="auto">{redactSecret(result.detail, draft.token)}</bdi>
                </span>
              ) : null}
            </Banner>
          )}

          {failed && (
            <div>
              <Button
                variant="ghost"
                data-action="connection.use-local"
                data-testid={`${testIdPrefix}-connection-use-local`}
                onClick={onUseLocal}
              >
                {t('onboarding.connection_use_local')}
              </Button>
            </div>
          )}
        </div>
      )}

      {footer && <div className="v3-connection-footer">{footer}</div>}
    </div>
  );
}
