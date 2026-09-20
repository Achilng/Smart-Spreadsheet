/** Split list inputs, preserving first occurrence, spelling and case. */
export function splitListText(value: string): string[] {
  return [...new Set(value.split(/[,，\n\r]/).map(item => item.trim()).filter(Boolean))];
}
