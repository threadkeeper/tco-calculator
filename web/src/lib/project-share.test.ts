import { describe, expect, it } from 'vitest';
import { projectShareFromFragment, projectShareUrl } from './project-share';

const credentials = {
  share_id: '019c6b68-9d29-7ff0-9f3b-4b2ad5d3ca74',
  secret: 'e289e1d7-1f48-49e9-a14d-61fa1b93dbba'
};

describe('project share links', () => {
  it('places credentials only in the URL fragment', () => {
    const sharedUrl = new URL(projectShareUrl('https://tco.example.test/', credentials));

    expect(sharedUrl.pathname).toBe('/');
    expect(sharedUrl.search).toBe('');
    expect(sharedUrl.hash).toContain('share=');
    expect(projectShareFromFragment(sharedUrl.hash)).toEqual(credentials);
  });

  it('rejects missing, malformed, and extra credential fields', () => {
    expect(projectShareFromFragment('')).toBeNull();
    expect(projectShareFromFragment('#share=not-a-credential')).toBeNull();
    expect(
      projectShareFromFragment(`#share=${credentials.share_id}.${credentials.secret}.extra`)
    ).toBeNull();
  });
});
