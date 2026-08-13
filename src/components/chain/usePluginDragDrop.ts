import { useEffect, useRef, useState } from 'react';
import type { MessageInstance } from 'antd/es/message/interface';
import type { PointerEvent as ReactPointerEvent } from 'react';
import type { PluginInstanceInfo } from '../../lib/types';

interface UsePluginDragDropOptions {
  pluginChain: PluginInstanceInfo[];
  isChainInitializing: boolean;
  isDeleteAllBusy: boolean;
  reorderChain: (fromIndex: number, toIndex: number) => Promise<void>;
  swapChain: (firstIndex: number, secondIndex: number) => Promise<void>;
  messageApi: MessageInstance;
}

/**
 * Pointer-based drag session for reordering/swapping plugin cards
 * (more reliable than HTML5 DnD inside the Tauri WebView).
 */
export function usePluginDragDrop({
  pluginChain,
  isChainInitializing,
  isDeleteAllBusy,
  reorderChain,
  swapChain,
  messageApi,
}: UsePluginDragDropOptions) {
  // draggedIndex: which card is being dragged
  // insertBefore: index BEFORE which the dragged card will be inserted
  //               (0 = before first, pluginChain.length = after last)
  const [draggedIndex, setDraggedIndex] = useState<number | null>(null);
  const [insertBefore, setInsertBefore] = useState<number | null>(null);
  const [swapTargetIndex, setSwapTargetIndex] = useState<number | null>(null);
  const [dragPointer, setDragPointer] = useState<{ x: number; y: number } | null>(null);
  const [dragLabel, setDragLabel] = useState('');
  const draggingRef = useRef(false);
  const draggedIndexRef = useRef<number | null>(null);
  const insertBeforeRef = useRef<number | null>(null);
  const swapTargetIndexRef = useRef<number | null>(null);

  useEffect(() => {
    insertBeforeRef.current = insertBefore;
  }, [insertBefore]);

  const startPointerDrag = (e: ReactPointerEvent, index: number) => {
    if (isChainInitializing || isDeleteAllBusy) return;
    if (e.button !== 0) return;
    e.preventDefault();

    draggingRef.current = true;
    draggedIndexRef.current = index;
    setDragLabel(pluginChain[index]?.name ?? 'Plugin');
    setDragPointer({ x: e.clientX, y: e.clientY });
    setDraggedIndex(index);
    setInsertBefore(null);
    setSwapTargetIndex(null);
    swapTargetIndexRef.current = null;

    const onPointerMove = (ev: globalThis.PointerEvent) => {
      if (draggedIndexRef.current === null) return;
      setDragPointer({ x: ev.clientX, y: ev.clientY });

      const el = document.elementFromPoint(ev.clientX, ev.clientY) as HTMLElement | null;

      // Check if hovering over arrow element
      const arrowEl = el?.closest('[data-plugin-arrow]') as HTMLElement | null;
      if (arrowEl) {
        const arrowPosRaw = arrowEl.dataset.pluginArrowPos;
        if (arrowPosRaw != null) {
          const pos = Number(arrowPosRaw);
          if (Number.isFinite(pos) && insertBeforeRef.current !== pos) {
            insertBeforeRef.current = pos;
            setInsertBefore(pos);
          }
        }
        if (swapTargetIndexRef.current !== null) {
          swapTargetIndexRef.current = null;
          setSwapTargetIndex(null);
        }
        return;
      }

      const cardEl = el?.closest('[data-plugin-card-index]') as HTMLElement | null;
      if (!cardEl) {
        if (insertBeforeRef.current !== null) {
          insertBeforeRef.current = null;
          setInsertBefore(null);
        }
        if (swapTargetIndexRef.current !== null) {
          swapTargetIndexRef.current = null;
          setSwapTargetIndex(null);
        }
        return;
      }

      const indexRaw = cardEl.dataset.pluginCardIndex;
      if (indexRaw == null) return;
      const cardIndex = Number(indexRaw);
      if (!Number.isFinite(cardIndex)) return;

      const rect = cardEl.getBoundingClientRect();

      // Check if pointer is within reasonable Y range of the card (with tolerance for multi-row layouts)
      const tolerance = 60;
      const isWithinVerticalBounds =
        ev.clientY >= rect.top - tolerance &&
        ev.clientY <= rect.bottom + tolerance;

      if (!isWithinVerticalBounds) {
        if (insertBeforeRef.current !== null) {
          insertBeforeRef.current = null;
          setInsertBefore(null);
        }
        if (swapTargetIndexRef.current !== null) {
          swapTargetIndexRef.current = null;
          setSwapTargetIndex(null);
        }
        return;
      }

      const from = draggedIndexRef.current;
      if (from === null) return;

      // Hovering a card means swap target. Hovering an arrow means insert target.
      if (insertBeforeRef.current !== null) {
        insertBeforeRef.current = null;
        setInsertBefore(null);
      }
      const nextSwapTarget = cardIndex === from ? null : cardIndex;
      if (swapTargetIndexRef.current !== nextSwapTarget) {
        swapTargetIndexRef.current = nextSwapTarget;
        setSwapTargetIndex(nextSwapTarget);
      }
    };

    const stopPointerDrag = async () => {
      window.removeEventListener('pointermove', onPointerMove);
      window.removeEventListener('pointerup', onPointerUp);
      window.removeEventListener('pointercancel', onPointerCancel);

      const from = draggedIndexRef.current;
      const pos = insertBeforeRef.current;
      const swapTo = swapTargetIndexRef.current;

      draggingRef.current = false;
      draggedIndexRef.current = null;
      insertBeforeRef.current = null;
      swapTargetIndexRef.current = null;
      setDragPointer(null);
      setDragLabel('');
      setDraggedIndex(null);
      setInsertBefore(null);
      setSwapTargetIndex(null);

      if (from === null) return;

      try {
        if (swapTo !== null && swapTo !== from) {
          await swapChain(from, swapTo);
          messageApi.success('Plugins swapped');
          return;
        }

        if (pos === null) return;
        const to = pos > from ? pos - 1 : pos;
        if (from === to) return;

        await reorderChain(from, to);
        messageApi.success('Plugin order updated');
      } catch (error) {
        messageApi.error('Failed to reorder plugins');
        console.error(error);
      }
    };

    const onPointerUp = () => { void stopPointerDrag(); };
    const onPointerCancel = () => { void stopPointerDrag(); };

    window.addEventListener('pointermove', onPointerMove);
    window.addEventListener('pointerup', onPointerUp);
    window.addEventListener('pointercancel', onPointerCancel);
  };

  const showInsertAt = (pos: number) =>
    draggedIndex !== null &&
    insertBefore === pos &&
    insertBefore !== draggedIndex &&
    insertBefore !== draggedIndex + 1;

  return {
    draggedIndex,
    swapTargetIndex,
    dragPointer,
    dragLabel,
    draggingRef,
    startPointerDrag,
    showInsertAt,
  };
}
