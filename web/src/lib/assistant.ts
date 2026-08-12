import { asRecord, readString, requestJson, requestJsonResponse, type JsonRecord } from '$lib/api';
import type { components } from '$lib/api/generated';

export const MAX_ASSISTANT_QUESTION_CHARACTERS = 1_000;
export const MAX_ASSISTANT_IMAGE_BYTES = 10 * 1024 * 1024;
export const ASSISTANT_IMAGE_MEDIA_TYPES = ['image/jpeg', 'image/png'] as const;

export type AssistantHelpRequest = components['schemas']['AssistantHelpRequest'];
export type AssistantHelpResponse = components['schemas']['AssistantHelpResponse'];
export type AssistantHelpReference = components['schemas']['AssistantHelpReference'];
export type AssistantTurnRequest = components['schemas']['AssistantTurnRequest'];
export type AssistantTurnResponse = components['schemas']['AssistantTurnResponse'];
export type AssistantImageResponse = components['schemas']['AssistantImageResponse'];
export type AssistantProjectPatch = components['schemas']['AssistantProjectPatch'];
export type AssistantProjectPatchProposal = components['schemas']['AssistantProjectPatchProposal'];
export type AssistantActionRequest = components['schemas']['AssistantActionRequest'];
export type AssistantActionResult = { document: JsonRecord; etag: string };

export function validateAssistantQuestion(question: string): string {
  const normalized = question.trim();
  const characterCount = Array.from(normalized).length;
  if (characterCount < 1 || characterCount > MAX_ASSISTANT_QUESTION_CHARACTERS) {
    throw new Error(
      `Question must contain 1 to ${MAX_ASSISTANT_QUESTION_CHARACTERS.toLocaleString('en-US')} characters.`
    );
  }
  return normalized;
}

function parseAssistantResponse(value: unknown, maximumReferences: number): AssistantHelpResponse {
  const response = asRecord(value);
  const answer = readString(response, 'answer');
  if (answer === null) {
    throw new Error('The assistant response was not recognized.');
  }

  return { answer, references: parseReferences(response, maximumReferences) };
}

function parseReferences(
  response: JsonRecord | null,
  maximumReferences: number
): AssistantHelpReference[] {
  const rawReferences = response?.references;
  if (!Array.isArray(rawReferences) || rawReferences.length > maximumReferences) {
    throw new Error('The assistant response was not recognized.');
  }

  return rawReferences.map((value): AssistantHelpReference => {
    const reference = asRecord(value);
    const controlId = readString(reference, 'control_id');
    const label = readString(reference, 'label');
    if (controlId === null || label === null) {
      throw new Error('The assistant response was not recognized.');
    }
    return { control_id: controlId, label };
  });
}

export function parseAssistantHelpResponse(value: unknown): AssistantHelpResponse {
  return parseAssistantResponse(value, 3);
}

export function parseAssistantTurnResponse(value: unknown): AssistantTurnResponse {
  const response = asRecord(value);
  const answer = readString(response, 'answer');
  if (answer === null || !response || !Object.hasOwn(response, 'proposal')) {
    throw new Error('The assistant response was not recognized.');
  }
  return {
    answer,
    references: parseReferences(response, 24),
    proposal: parseProposal(response.proposal)
  };
}

export function parseAssistantImageResponse(value: unknown): AssistantImageResponse {
  const response = asRecord(value);
  const answer = readString(response, 'answer');
  if (answer === null || !response || !Object.hasOwn(response, 'proposal')) {
    throw new Error('The assistant image response was not recognized.');
  }
  return {
    answer,
    proposal: parseProposal(response.proposal),
    omissions: parseNotes(response.omissions),
    uncertainties: parseNotes(response.uncertainties)
  };
}

export async function requestAssistantHelp(
  question: string,
  signal?: AbortSignal
): Promise<AssistantHelpResponse> {
  const request: AssistantHelpRequest = { question: validateAssistantQuestion(question) };
  const response = await requestJson('/api/v1/assistant/help', {
    method: 'POST',
    body: JSON.stringify(request),
    cache: 'no-store',
    signal
  });
  return parseAssistantHelpResponse(response);
}

export async function requestAssistantTurn(
  question: string,
  projectId: string | null,
  signal?: AbortSignal
): Promise<AssistantTurnResponse> {
  const request: AssistantTurnRequest = {
    question: validateAssistantQuestion(question),
    project_id: projectId
  };
  const response = await requestJson('/api/v1/assistant/turn', {
    method: 'POST',
    body: JSON.stringify(request),
    cache: 'no-store',
    signal
  });
  return parseAssistantTurnResponse(response);
}

export async function requestAssistantImage(
  image: File,
  projectId: string,
  signal?: AbortSignal
): Promise<AssistantImageResponse> {
  validateAssistantImage(image);
  const response = await requestJson('/api/v1/assistant/image', {
    method: 'POST',
    headers: {
      'content-type': image.type,
      'x-tco-project-id': projectId
    },
    body: image,
    cache: 'no-store',
    signal
  });
  return parseAssistantImageResponse(response);
}

export function validateAssistantImage(image: File): File {
  if (!ASSISTANT_IMAGE_MEDIA_TYPES.some((mediaType) => mediaType === image.type)) {
    throw new Error('Choose a JPEG or PNG image.');
  }
  if (image.size < 1 || image.size > MAX_ASSISTANT_IMAGE_BYTES) {
    throw new Error('Image must be no larger than 10 MiB.');
  }
  return image;
}

export async function executeAssistantAction(
  proposal: AssistantProjectPatchProposal,
  signal?: AbortSignal
): Promise<AssistantActionResult> {
  const request: AssistantActionRequest = {
    proposal_id: proposal.proposal_id,
    action: proposal.action,
    project_id: proposal.project_id,
    expected_etag: proposal.expected_etag,
    patch: proposal.patch
  };
  const response = await requestJsonResponse('/api/v1/assistant/actions', {
    method: 'POST',
    headers: { 'x-tco-action-confirmation': 'apply_project_patch' },
    body: JSON.stringify(request),
    cache: 'no-store',
    signal
  });
  const document = asRecord(response.payload);
  if (!document || readString(document, 'id') !== proposal.project_id || response.etag === null) {
    throw new Error('The updated project response was not recognized.');
  }
  return { document, etag: response.etag };
}

function parseProposal(value: unknown): AssistantProjectPatchProposal | null {
  if (value === null) return null;
  const proposal = asRecord(value);
  const proposalId = readString(proposal, 'proposal_id');
  const action = readString(proposal, 'action');
  const projectId = readString(proposal, 'project_id');
  const expectedEtag = readString(proposal, 'expected_etag');
  const patch = parsePatch(proposal?.patch);
  const rawChanges = proposal?.changes;
  if (
    !proposalId?.match(/^sha256:[0-9a-f]{64}$/) ||
    action !== 'apply_project_patch' ||
    !projectId ||
    !expectedEtag ||
    patch === null ||
    !Array.isArray(rawChanges) ||
    rawChanges.length < 1 ||
    rawChanges.length > 500
  ) {
    throw new Error('The assistant proposal was not recognized.');
  }
  const changes = rawChanges.map((value) => {
    const change = asRecord(value);
    const pointer = readString(change, 'pointer');
    if (
      !change ||
      !pointer ||
      !Object.hasOwn(change, 'before') ||
      !Object.hasOwn(change, 'after')
    ) {
      throw new Error('The assistant proposal was not recognized.');
    }
    return { pointer, before: change.before, after: change.after };
  });
  return {
    proposal_id: proposalId,
    action,
    project_id: projectId,
    expected_etag: expectedEtag,
    patch,
    changes
  };
}

function parsePatch(value: unknown): AssistantProjectPatch | null {
  const patch = asRecord(value);
  if (!patch) return null;
  const allowed = new Set(['name', 'description', 'settings', 'resources']);
  if (Object.keys(patch).some((key) => !allowed.has(key))) return null;
  if (Object.hasOwn(patch, 'name') && typeof patch.name !== 'string') return null;
  if (
    Object.hasOwn(patch, 'description') &&
    patch.description !== null &&
    typeof patch.description !== 'string'
  ) {
    return null;
  }
  if (Object.hasOwn(patch, 'settings') && asRecord(patch.settings) === null) return null;
  if (
    Object.hasOwn(patch, 'resources') &&
    (!Array.isArray(patch.resources) ||
      patch.resources.length > 100 ||
      patch.resources.some((resource) => asRecord(resource) === null))
  ) {
    return null;
  }
  return structuredClone(patch) as AssistantProjectPatch;
}

function parseNotes(value: unknown): string[] {
  if (!Array.isArray(value) || value.length > 100) {
    throw new Error('The assistant image response was not recognized.');
  }
  return value.map((note) => {
    if (typeof note !== 'string' || note.length < 1 || Array.from(note).length > 500) {
      throw new Error('The assistant image response was not recognized.');
    }
    return note;
  });
}
