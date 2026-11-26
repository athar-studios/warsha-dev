/* apps/desktop/src/v3/pages/settings/sections/ConnectionSection.tsx — Settings → Connection.
 * Distinct from ConnectionsSection.tsx (MCP servers): this is where Warsha itself runs — this
 * device, or a warshad reachable at an address. The token is never read back to the screen; only
 * whether one is stored.
 */
import { useT } from '@warsha/i18n/v3';
import { Banner, Button, SettingsSection } from '@warsha/ui/v3';
import React, { useCallback, useEffect, useState } from 'react';

import { useReloadClient } from '../../../app/ClientContext';
import { useDataReady } from '../../../app/DataReadyContext';
import {
  ConnectionMethodForm,
  isTauriRuntime,
  readConnectionConfig,
  saveConnection,
  useConnectionMethod,
  type ConnectionStoredState,
} from '../../../components/ConnectionMethodForm';

export function ConnectionSection(): React.ReactElement {
  const t = useT();
  const { setReady } = useDataReady();
  const reloadClient = useReloadClient();
  const connection = useConnectionMethod();
  const { setMode: setConnectionMode, setUrl: setConnectionUrl } = connection;

  const [stored, setStored] = useState<ConnectionStoredState | null>(null);
  const [loading, setLoading] = useState<boolean>(true);
  const [saving, setSaving] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState<boolean>(false);

  const load = useCallback(async (): Promise<void> => {
    setLoading(true);
    const next = await readConnectionConfig();
    setStored(next);
    setConnectionMode(next.mode);
    if (next.url) setConnectionUrl(next.url);
    setLoading(false);
  }, [setConnectionMode, setConnectionUrl]);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    setReady(!loading);
    return () => {
      setReady(true);
    };
  }, [loading, setReady]);

  const handleSave = useCallback(async (): Promise<void> => {
    if (connection.draft.mode === 'server' && !connection.isVerified) {
      setError(t('onboarding.connection_test_required'));
      return;
    }
    setError(null);
    setSaved(false);
    setSaving(true);
    try {
      await saveConnection({
        mode: connection.draft.mode,
        url: connection.draft.url,
        token: connection.draft.token,
        wsUrl: connection.result?.wsUrl ?? '',
      });
      await reloadClient();
      const next = await readConnectionConfig();
      setStored(next);
      setSaved(true);
    } catch {
      setError(t('onboarding.connection_save_failed'));
    } finally {
      setSaving(false);
    }
  }, [connection, reloadClient, t]);

  const inShell = isTauriRuntime();
  const connectionBlocked = connection.draft.mode === 'server' && !connection.isVerified;
  const isServer = stored?.mode === 'server';

  return (
    <div data-testid="settings-section-connection">
      <SettingsSection
        title={t('settings.section.connection')}
        description={t('settings.section.connection.desc')}
        scope={<span className="v3-settings-scope">{t('settings.scope.device')}</span>}
      >
        <div className="v3-settings-stack">
          <div className="v3-settings-row" data-testid="settings-connection-current">
            <div className="v3-settings-row-main">
              <div className="v3-settings-row-title" data-testid="settings-connection-mode">
                {isServer
                  ? t('settings.section.connection.current_server')
                  : t('settings.section.connection.current_local')}
              </div>
              {isServer && stored && stored.url ? (
                <div className="v3-settings-row-subtitle">
                  <span className="v3-settings-row-meta">
                    {t('settings.section.connection.address')}:{' '}
                  </span>
                  <bdi dir="ltr" data-testid="settings-connection-address">
                    {stored.url}
                  </bdi>
                </div>
              ) : null}
              {isServer ? (
                <div className="v3-settings-row-meta" data-testid="settings-connection-token-state">
                  {stored?.tokenSet
                    ? t('settings.section.connection.token_set')
                    : t('settings.section.connection.token_unset')}
                </div>
              ) : null}
            </div>
          </div>

          {!inShell && (
            <Banner variant="info" data-testid="settings-connection-browser-notice">
              {t('settings.section.connection.browser_notice')}
            </Banner>
          )}

          {error && (
            <Banner variant="danger" data-testid="settings-connection-error">
              <bdi dir="auto">{error}</bdi>
            </Banner>
          )}

          {saved && (
            <Banner variant="success" data-testid="settings-connection-saved">
              {t('settings.section.connection.saved')}
            </Banner>
          )}

          <ConnectionMethodForm
            draft={connection.draft}
            result={connection.result}
            testing={connection.testing}
            testIdPrefix="settings"
            disabled={saving}
            onModeChange={setConnectionMode}
            onUrlChange={setConnectionUrl}
            onTokenChange={connection.setToken}
            onTest={() => {
              void connection.test();
            }}
            onUseLocal={connection.useLocal}
            footer={
              <>
                <Button
                  variant="primary"
                  loading={saving}
                  disabled={connectionBlocked || !inShell}
                  data-action="settings.connection.save"
                  data-testid="settings-connection-save"
                  onClick={() => {
                    void handleSave();
                  }}
                >
                  {t('settings.section.connection.save')}
                </Button>
                <span className="v3-settings-row-meta">
                  {t('settings.section.connection.reconnect_note')}
                </span>
              </>
            }
          />
        </div>
      </SettingsSection>
    </div>
  );
}
