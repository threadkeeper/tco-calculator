import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  MAX_ASSISTANT_QUESTION_CHARACTERS,
  executeAssistantAction,
  parseAssistantImageResponse,
  parseAssistantHelpResponse,
  parseAssistantTurnResponse,
  requestAssistantHelp,
  requestAssistantImage,
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
      new Response(JSON.stringify({ answer: 'Reviewed answer', references: [], proposal: null }), {
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

  it('preserves a closed validated proposal from a model turn', () => {
    const proposal = projectProposal();

    expect(
      parseAssistantTurnResponse({ answer: 'Review this change.', references: [], proposal })
        .proposal
    ).toEqual(proposal);
    expect(() =>
      parseAssistantTurnResponse({
        answer: 'Bad proposal',
        references: [],
        proposal: { ...proposal, action: 'delete_project' }
      })
    ).toThrow('proposal was not recognized');
  });

  it('preserves a validated unsaved project draft from a model turn', () => {
    const proposal = newProjectDraftProposal();

    expect(
      parseAssistantTurnResponse({ answer: 'Open this draft.', references: [], proposal }).proposal
    ).toEqual(proposal);
    expect(() =>
      parseAssistantTurnResponse({
        answer: 'Bad draft',
        references: [],
        proposal: { ...proposal, project: { name: 'Missing project settings' } }
      })
    ).toThrow('proposal was not recognized');
  });

  it('uploads only bounded JPEG or PNG bytes with the host-selected project ID', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          answer: 'Review the extracted values.',
          proposal: null,
          omissions: ['Unsupported field'],
          uncertainties: []
        }),
        { status: 200, headers: { 'content-type': 'application/json' } }
      )
    );
    vi.stubGlobal('fetch', fetchMock);
    const image = new File([new Uint8Array([0xff, 0xd8, 0xff])], 'inventory.jpg', {
      type: 'image/jpeg'
    });

    await expect(requestAssistantImage(image, 'project-id')).resolves.toMatchObject({
      omissions: ['Unsupported field']
    });
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/v1/assistant/image');
    const request = fetchMock.mock.calls[0]?.[1] as RequestInit;
    expect(request.body).toBe(image);
    expect(new Headers(request.headers).get('content-type')).toBe('image/jpeg');
    expect(new Headers(request.headers).get('x-tco-project-id')).toBe('project-id');
    expect(JSON.stringify(request.headers)).not.toContain('inventory.jpg');

    await expect(
      requestAssistantImage(new File(['bad'], 'bad.gif', { type: 'image/gif' }), 'project-id')
    ).rejects.toThrow('JPEG or PNG');
  });

  it('omits the project header when an image starts a new browser draft', async () => {
    const proposal = newProjectDraftProposal();
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          answer: 'Open the extracted draft.',
          proposal,
          omissions: [],
          uncertainties: []
        }),
        { status: 200, headers: { 'content-type': 'application/json' } }
      )
    );
    vi.stubGlobal('fetch', fetchMock);
    const image = new File([new Uint8Array([0xff, 0xd8, 0xff])], 'inventory.jpg', {
      type: 'image/jpeg'
    });

    await expect(requestAssistantImage(image, null)).resolves.toMatchObject({ proposal });
    const request = fetchMock.mock.calls[0]?.[1] as RequestInit;
    expect(new Headers(request.headers).get('content-type')).toBe('image/jpeg');
    expect(new Headers(request.headers).has('x-tco-project-id')).toBe(false);
  });

  it('requires bounded typed image omissions and uncertainties', () => {
    expect(() =>
      parseAssistantImageResponse({
        answer: 'Incomplete',
        proposal: null,
        omissions: ['a'.repeat(501)],
        uncertainties: []
      })
    ).toThrow('image response was not recognized');
  });

  it('executes exactly the reviewed proposal with a dedicated confirmation header', async () => {
    const proposal = projectProposal();
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ id: proposal.project_id, name: 'Imported estimate' }), {
        status: 200,
        headers: { 'content-type': 'application/json', etag: '"etag-2"' }
      })
    );
    vi.stubGlobal('fetch', fetchMock);

    await expect(executeAssistantAction(proposal)).resolves.toMatchObject({ etag: '"etag-2"' });
    const request = fetchMock.mock.calls[0]?.[1] as RequestInit;
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/v1/assistant/actions');
    expect(new Headers(request.headers).get('x-tco-action-confirmation')).toBe(
      'apply_project_patch'
    );
    expect(JSON.parse(String(request.body))).toEqual({
      proposal_id: proposal.proposal_id,
      action: proposal.action,
      project_id: proposal.project_id,
      expected_etag: proposal.expected_etag,
      patch: proposal.patch
    });
  });
});

function projectProposal() {
  return {
    proposal_id: `sha256:${'a'.repeat(64)}`,
    action: 'apply_project_patch' as const,
    project_id: '11111111-1111-1111-1111-111111111111',
    expected_etag: '"etag-1"',
    patch: { name: 'Imported estimate' },
    changes: [{ pointer: '/name', before: 'Estimate', after: 'Imported estimate' }]
  };
}

function newProjectDraftProposal() {
  return {
    proposal_id: `sha256:${'b'.repeat(64)}`,
    action: 'open_project_draft' as const,
    project: {
      name: 'Imported inventory',
      description: null,
      settings: {
        project_type: 'on_prem' as const,
        aws_region: null,
        azure_region: 'swedencentral',
        currency: 'USD' as const,
        source_compute_discount: '0',
        source_license_discount: '0',
        source_storage_discount: '0',
        azure_compute_discount: '0',
        azure_license_discount: '0',
        azure_storage_discount: '0',
        selected_parity_adjustment: '0',
        default_annual_hours: '8760',
        default_mi_purchase_option: 'ahb' as const,
        enterprise_license_sa_usd_per_two_core_pack: '20557',
        standard_license_sa_usd_per_two_core_pack: '5363',
        remaining_coverage_months: 12 as const,
        electricity_rate_usd_per_kwh: '0'
      },
      resources: [],
      aws_price_snapshot_id: null,
      azure_price_snapshot_id: null
    }
  };
}
