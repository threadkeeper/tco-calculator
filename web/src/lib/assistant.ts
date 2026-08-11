import { asRecord, readString, requestJson } from '$lib/api';
import type { components } from '$lib/api/generated';

export const MAX_ASSISTANT_QUESTION_CHARACTERS = 1_000;

export type AssistantHelpRequest = components['schemas']['AssistantHelpRequest'];
export type AssistantHelpResponse = components['schemas']['AssistantHelpResponse'];
export type AssistantHelpReference = components['schemas']['AssistantHelpReference'];

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

export function parseAssistantHelpResponse(value: unknown): AssistantHelpResponse {
  const response = asRecord(value);
  const answer = readString(response, 'answer');
  const rawReferences = response?.references;
  if (answer === null || !Array.isArray(rawReferences) || rawReferences.length > 3) {
    throw new Error('The assistant response was not recognized.');
  }

  const references = rawReferences.map((value): AssistantHelpReference => {
    const reference = asRecord(value);
    const controlId = readString(reference, 'control_id');
    const label = readString(reference, 'label');
    if (controlId === null || label === null) {
      throw new Error('The assistant response was not recognized.');
    }
    return { control_id: controlId, label };
  });

  return { answer, references };
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
