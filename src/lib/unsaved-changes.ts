const drafts = new Set<() => boolean>();
let prompt: (() => Promise<boolean>) | undefined;

export function registerUnsavedChanges(isDirty: () => boolean) {
  drafts.add(isDirty);
  return () => {
    drafts.delete(isDirty);
  };
}

export function hasUnsavedChanges() {
  return [...drafts].some((isDirty) => isDirty());
}

export function setUnsavedChangesPrompt(value?: () => Promise<boolean>) {
  prompt = value;
}

export async function confirmUnsavedChanges(isDirty = hasUnsavedChanges) {
  return !isDirty() || (prompt ? await prompt() : false);
}
