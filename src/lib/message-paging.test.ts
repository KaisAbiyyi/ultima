import { describe, expect, it } from 'vitest';
import { READER_PAGE_CHARS, ROW_PREVIEW_CHARS, rowPreview, readerPage } from './message-paging';
describe('NC-08 DOM text is bounded', () => {
  it('caps visible text even when the retained answer is 2 MiB', () => {
    expect(rowPreview('x'.repeat(2 * 1024 * 1024))).toHaveLength(ROW_PREVIEW_CHARS + 1);
    expect(rowPreview('short')).toBe('short');
  });
  it('lazy pages preserve code and markdown source exactly', () => {
    const source = '# heading\ncode block\n'.repeat(1500);
    const count = readerPage(source, 0).pages;
    const parts = Array.from({length:count}, (_,i)=>readerPage(source,i).text);
    expect(parts.join('')).toBe(source);
    expect(parts.every(part=>part.length<=READER_PAGE_CHARS)).toBe(true);
    expect(readerPage(source,-1).page).toBe(0);
    expect(readerPage(source,Infinity).page).toBe(0);
    expect(readerPage(source,9999).page).toBe(count-1);
  });
  it('empty data never creates an invalid page', () => {
    expect(readerPage('',0)).toEqual({page:0,pages:1,text:'',hasPrevious:false,hasNext:false});
  });
});
