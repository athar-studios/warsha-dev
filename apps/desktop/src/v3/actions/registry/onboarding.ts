import type { ActionDef } from '../types';

export const ACTIONS: readonly ActionDef[] = [
  {
    id: 'onboarding.language.pick',
    labelKey: 'onboarding.action_language_pick',
    effect: 'state',
  },
  {
    id: 'onboarding.provider.pick',
    labelKey: 'onboarding.action_provider_pick',
    effect: 'state',
  },
  {
    id: 'onboarding.provider.save-test',
    labelKey: 'onboarding.save_test',
    effect: 'rpc',
    rpc: ['providers.add', 'providers.test'],
  },
  {
    id: 'onboarding.models.fetch',
    labelKey: 'onboarding.fetch_models',
    effect: 'rpc',
    rpc: ['providers.check_models'],
  },
  {
    id: 'onboarding.model.use',
    labelKey: 'onboarding.use_model',
    effect: 'rpc',
    rpc: ['models.add', 'roles.set'],
  },
  {
    id: 'onboarding.back',
    labelKey: 'onboarding.back',
    effect: 'state',
  },
  {
    id: 'onboarding.next',
    labelKey: 'onboarding.next',
    effect: 'state',
  },
  // Step order: language → connection → provider → github → idea.
  // The three `connection.*` ids live on the shared ConnectionMethodForm, so onboarding and
  // Settings → Connection use the same literals. `connection.test` calls the shell command
  // `test_daemon_connection`, not a daemon method, so it carries no rpc.
  { id: 'connection.mode.pick', labelKey: 'onboarding.action_connection_pick', effect: 'state' },
  { id: 'connection.test', labelKey: 'onboarding.action_connection_test', effect: 'state' },
  { id: 'connection.use-local', labelKey: 'onboarding.action_connection_local', effect: 'state' },
  { id: 'onboarding.provider.skip', labelKey: 'onboarding.provider_skip', effect: 'state' },
  {
    id: 'onboarding.github.connect',
    labelKey: 'onboarding.action_github_connect',
    effect: 'rpc',
    rpc: ['integrations.connect', 'integrations.test'],
  },
  { id: 'onboarding.github.skip', labelKey: 'onboarding.action_skip', effect: 'state' },
  {
    id: 'onboarding.start',
    labelKey: 'onboarding.start_idea',
    effect: 'rpc',
    rpc: ['projects.create', 'runs.start'],
    route: '/p/:id',
  },
  {
    id: 'onboarding.later',
    labelKey: 'onboarding.later',
    effect: 'nav',
    route: '/',
  },
  {
    id: 'onboarding.retry',
    labelKey: 'onboarding.error_retry',
    effect: 'rpc',
    rpc: ['providers.list'],
  },
];
