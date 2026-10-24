export interface WindowRange {
  start: number;
  end: number;
}

/** Fixed-height row window; caps rendered DOM independently of conversation size. */
export function visibleRange(
  count: number,
  scrollTop: number,
  viewportHeight: number,
  rowHeight: number,
  overscan = 6
): WindowRange {
  if (!Number.isFinite(count) || count < 0 || !Number.isInteger(count) ||
      !Number.isFinite(rowHeight) || rowHeight <= 0 ||
      !Number.isFinite(scrollTop) || !Number.isFinite(viewportHeight) ||
      !Number.isInteger(overscan) || overscan < 0) {
    throw new RangeError('Invalid virtualization arguments');
  }

  const top = Math.max(0, scrollTop);
  const height = Math.max(0, viewportHeight);
  const first = Math.floor(top / rowHeight);
  return {
    start: Math.min(count, Math.max(0, first - overscan)),
    end: Math.min(count, Math.max(0, Math.ceil((top + height) / rowHeight) + overscan))
  };
}
