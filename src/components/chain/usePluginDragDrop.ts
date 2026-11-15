import { useRef, useState } from 'react';
import type { MessageInstance } from 'antd/es/message/interface';
import type { KeyboardEvent as ReactKeyboardEvent, PointerEvent as ReactPointerEvent, RefObject } from 'react';
import type { PluginInstanceInfo } from '../../lib/types';
import { useTranslation } from '../../i18n';
import { resolveDropTarget, slotToIndex, type Rect } from './dropTarget';

interface UsePluginDragDropOptions {
  pluginChain: PluginInstanceInfo[];
  rowRef: RefObject<HTMLElement | null>;
  isChainInitializing: boolean;
  isDeleteAllBusy: boolean;
  reorderChain: (fromIndex: number, toIndex: number) => Promise<void>;
  swapChain: (firstIndex: number, secondIndex: number) => Promise<void>;
  fetchChain: () => Promise<void>;
  messageApi: MessageInstance;
}

/** Pointer slack around the card row before a drop counts as "outside" (cancel). */
const OUTSIDE_MARGIN = 24;

/**
 * Pointer-based drag session for reordering/swapping plugin cards
 * (more reliable than HTML5 DnD inside the Tauri WebView).
 */
export function usePluginDragDrop({
  pluginChain,
  rowRef,
  isChainInitializing,
  isDeleteAllBusy,
  reorderChain,
  swapChain,
  fetchChain,
  messageApi,
}: UsePluginDragDropOptions) {
  const { t } = useTranslation();
  const [draggedIndex, setDraggedIndex] = useState<number | null>(null);
  // Insert slot: 0 = before first card, pluginChain.length = after last.
  const [insertBefore, setInsertBefore] = useState<number | null>(null);
  const [swapTargetIndex, setSwapTargetIndex] = useState<number | null>(null);
  const [dragPointer, setDragPointer] = useState<{ x: number; y: number } | null>(null);
  const [dragLabel, setDragLabel] = useState('');
  const draggingRef = useRef(false);
  const insertBeforeRef = useRef<number | null>(null);
  const swapTargetIndexRef = useRef<number | null>(null);

  const setTarget = (slot: number | null, swap: number | null) => {
    if (insertBeforeRef.current !== slot) {
      insertBeforeRef.current = slot;
      setInsertBefore(slot);
    }
    if (swapTargetIndexRef.current !== swap) {
      swapTargetIndexRef.current = swap;
      setSwapTargetIndex(swap);
    }
  };

  const commitReorder = async (from: number, to: number) => {
    try {
      await reorderChain(from, to);
      messageApi.success(t('chain.orderUpdated'));
    } catch (error) {
      messageApi.error(t('chain.reorderFailed'));
      console.error(error);
    }
  };

  const startPointerDrag = (e: ReactPointerEvent, index: number) => {
    if (isChainInitializing || isDeleteAllBusy || draggingRef.current) return;
    if (e.button !== 0) return;
    e.preventDefault();
    // Keep receiving move/up even if the pointer leaves the WebView.
    try { (e.currentTarget as Element).setPointerCapture(e.pointerId); } catch { /* not capturable */ }

    const from = index;
    draggingRef.current = true;
    setDragLabel(pluginChain[index]?.name ?? 'Plugin');
    setDragPointer({ x: e.clientX, y: e.clientY });
    setDraggedIndex(index);
    setTarget(null, null);

    const onPointerMove = (ev: globalThis.PointerEvent) => {
      setDragPointer({ x: ev.clientX, y: ev.clientY });
      const row = rowRef.current;
      if (!row) return setTarget(null, null);

      const cards: Rect[] = [];
      row.querySelectorAll<HTMLElement>('[data-plugin-card-index]').forEach((el) => {
        cards[Number(el.dataset.pluginCardIndex)] = el.getBoundingClientRect();
      });
      const b = row.getBoundingClientRect();
      const bounds = {
        left: b.left - OUTSIDE_MARGIN,
        right: b.right + OUTSIDE_MARGIN,
        top: b.top - OUTSIDE_MARGIN,
        bottom: b.bottom + OUTSIDE_MARGIN,
      };

      const target = resolveDropTarget(cards, ev.clientX, ev.clientY, bounds);
      if (target?.kind === 'swap') setTarget(null, target.index === from ? null : target.index);
      else if (target?.kind === 'insert') setTarget(slotToIndex(from, target.slot) === null ? null : target.slot, null);
      else setTarget(null, null);
    };

    const endDrag = async (commit: boolean) => {
      window.removeEventListener('pointermove', onPointerMove);
      window.removeEventListener('pointerup', onPointerUp);
      window.removeEventListener('pointercancel', onPointerCancel);
      window.removeEventListener('keydown', onKeyDown);

      const slot = insertBeforeRef.current;
      const swapTo = swapTargetIndexRef.current;

      draggingRef.current = false;
      setTarget(null, null);
      setDragPointer(null);
      setDragLabel('');
      setDraggedIndex(null);

      try {
        if (!commit) return;
        if (swapTo !== null) {
          await swapChain(from, swapTo);
          messageApi.success(t('chain.pluginsSwapped'));
          return;
        }
        const to = slot === null ? null : slotToIndex(from, slot);
        if (to !== null) await commitReorder(from, to);
      } catch (error) {
        messageApi.error(t('chain.reorderFailed'));
        console.error(error);
      } finally {
        // plugin-chain-changed events are ignored mid-drag; resync so none are lost.
        void fetchChain();
      }
    };

    const onPointerUp = () => { void endDrag(true); };
    // Touch scroll/gesture takeover or Escape: cancel, never drop.
    const onPointerCancel = () => { void endDrag(false); };
    const onKeyDown = (ev: KeyboardEvent) => { if (ev.key === 'Escape') void endDrag(false); };

    window.addEventListener('pointermove', onPointerMove);
    window.addEventListener('pointerup', onPointerUp);
    window.addEventListener('pointercancel', onPointerCancel);
    window.addEventListener('keydown', onKeyDown);
  };

  /** Keyboard alternative on the focused drag handle: arrows move by one, Home/End to the ends. */
  const moveByKey = (e: ReactKeyboardEvent, index: number) => {
    if (isChainInitializing || isDeleteAllBusy || draggingRef.current) return;
    const last = pluginChain.length - 1;
    const to = e.key === 'ArrowLeft' || e.key === 'ArrowUp' ? index - 1
      : e.key === 'ArrowRight' || e.key === 'ArrowDown' ? index + 1
      : e.key === 'Home' ? 0
      : e.key === 'End' ? last
      : null;
    if (to === null) return;
    e.preventDefault();
    if (to < 0 || to > last || to === index) return;
    void commitReorder(index, to);
  };

  // Slots adjacent to the dragged card are no-ops, so never highlighted.
  const showInsertAt = (pos: number) => draggedIndex !== null && insertBefore === pos;

  return {
    draggedIndex,
    swapTargetIndex,
    dragPointer,
    dragLabel,
    draggingRef,
    startPointerDrag,
    moveByKey,
    showInsertAt,
  };
}
