import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  MAX_ASSISTANT_QUESTION_CHARACTERS,
  parseAssistantHelpResponse,
  requestAssistantHelp,
  requestAssistantTurn,
  validateAssistantQuestion
} from './assistant';

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('assistant help client', () => {
  it('trims bounded input and requests a non-cacheable deterministic answer', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          answer: 'Azure region selects the public Azure prices.',
          references: [{ control_id: 'project.azure-region', label: 'Azure region' }]
        }),
        { status: 200, headers: { 'content-type': 'application/json' } }
      )
    );
    vi.stubGlobal('fetch', fetchMock);

    await expect(requestAssistantHelp('  What does Azure region mean?  ')).resolves.toMatchObject({
      references: [{ control_id: 'project.azure-region' }]
    });

    expect(fetchMock).toHaveBeenCalledOnce();
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/v1/assistant/help');
    expect(fetchMock.mock.calls[0]?.[1]).toMatchObject({
      method: 'POST',
      body: JSON.stringify({ question: 'What does Azure region mean?' }),
      cache: 'no-store'
    });
  });

  it('counts Unicode characters and rejects empty or oversized questions', () => {
    expect(() => validateAssistantQuestion('   ')).toThrow('Question must contain');
    expect(() =>
      validateAssistantQuestion('😀'.repeat(MAX_ASSISTANT_QUESTION_CHARACTERS + 1))
    ).toThrow('Question must contain');
  });

  it('rejects malformed or excessive control references', () => {
    expect(() =>
      parseAssistantHelpResponse({ answer: 'Help', references: [{ control_id: 42, label: 'Bad' }] })
    ).toThrow('not recognized');
    expect(() =>
      parseAssistantHelpResponse({
        answer: 'Help',
        references: Array.from({ length: 4 }, (_, index) => ({
          control_id: `control.${index}`,
          label: `Control ${index}`
        }))
      })
    ).toThrow('not recognized');
  });

  it('sends authenticated turns with only the host-selected project identifier', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ answer: 'Reviewed answer', references: [] }), {
        status: 200,
        headers: { 'content-type': 'application/json' }
      })
    );
    vi.stubGlobal('fetch', fetchMock);

    await requestAssistantTurn('  Explain this estimate  ', 'project-id');

    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/v1/assistant/turn');
    expect(fetchMock.mock.calls[0]?.[1]).toMatchObject({
      method: 'POST',
      body: JSON.stringify({ question: 'Explain this estimate', project_id: 'project-id' }),
      cache: 'no-store'
    });
  });
});
