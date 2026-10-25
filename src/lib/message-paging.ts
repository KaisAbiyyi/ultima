/** Preview and source paging are bounded regardless of original message size. */
export const ROW_PREVIEW_CHARS = 320;
export const READER_PAGE_CHARS = 4096;
export function rowPreview(text: string): string {
  return text.length > ROW_PREVIEW_CHARS ? text.slice(0, ROW_PREVIEW_CHARS) + '…' : text;
}
export function readerPage(text: string, requested: number) {
  const pages = Math.max(1, Math.ceil(text.length / READER_PAGE_CHARS));
  const page = Number.isSafeInteger(requested) ? Math.max(0, Math.min(pages - 1, requested)) : 0;
  return {
    page, pages,
    text: text.slice(page * READER_PAGE_CHARS, (page + 1) * READER_PAGE_CHARS),
    hasPrevious: page > 0, hasNext: page < pages - 1
  };
}
