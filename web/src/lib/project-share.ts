import type { components } from '$lib/api/generated';

export type ProjectShareCredentials = components['schemas']['ProjectShareCredentials'];

const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export function projectShareUrl(currentUrl: string, credentials: ProjectShareCredentials): string {
  const url = new URL(currentUrl);
  url.hash = new URLSearchParams({
    share: `${credentials.share_id}.${credentials.secret}`
  }).toString();
  return url.toString();
}

export function projectShareFromFragment(fragment: string): ProjectShareCredentials | null {
  const encoded = fragment.startsWith('#') ? fragment.slice(1) : fragment;
  const value = new URLSearchParams(encoded).get('share');
  if (!value) return null;
  const parts = value.split('.');
  if (parts.length !== 2 || !parts.every((part) => UUID_PATTERN.test(part))) return null;
  return { share_id: parts[0], secret: parts[1] };
}
