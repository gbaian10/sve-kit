/** DOM id of one suggest option, shared by the list and the combobox's aria-activedescendant. */
export function optionId(listId: string, index: number): string {
  return `${listId}-option-${String(index)}`
}
