import { useEffect, useState, type RefObject } from 'react';

/**
 * Detects which arrow positions sit at a flex-wrap row break, so the caller
 * can render a distinct "turns down to the next row" connector there instead
 * of the normal left-to-right arrow, which otherwise looks like it snapped
 * to the wrong spot whenever the signal chain wraps.
 *
 * Walks the row container's direct children in DOM order and compares
 * offsetTop between consecutive children — robust to wherever the browser
 * actually decided to wrap, rather than assuming a fixed items-per-row count.
 */
export function useRowBreaks(containerRef: RefObject<HTMLElement | null>, deps: unknown[]): Set<number> {
  const [breaks, setBreaks] = useState<Set<number>>(new Set());

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const measure = () => {
      const children = Array.from(container.children) as HTMLElement[];
      const next = new Set<number>();
      let lastArrowPos: number | null = null;

      for (let i = 0; i < children.length; i++) {
        const el = children[i];
        const posAttr = el.dataset.plugincombinedpos ?? el.dataset.pluginArrowPos;
        if (posAttr != null) lastArrowPos = Number(posAttr);

        const prev = children[i - 1];
        if (!prev) continue;
        const broke = Math.abs(prev.offsetTop - el.offsetTop) > 4;
        if (!broke) continue;

        // The break lands on this child. If it carries an arrow position
        // itself, use that; otherwise (e.g. OUT wrapping alone) fall back to
        // the nearest preceding arrow, which is the connector actually
        // bridging the two rows.
        const pos = posAttr != null ? Number(posAttr) : lastArrowPos;
        if (pos != null) next.add(pos);
      }

      setBreaks((prev) => {
        if (prev.size === next.size && [...prev].every((p) => next.has(p))) return prev;
        return next;
      });
    };

    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(container);
    return () => ro.disconnect();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);

  return breaks;
}
