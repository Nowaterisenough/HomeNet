import { CSSProperties, PointerEvent, useCallback, useMemo, useRef, useState } from "react";

interface DragStart {
  pointerX: number;
  pointerY: number;
  offsetX: number;
  offsetY: number;
}

export function useDraggableModal() {
  const [offset, setOffset] = useState({ x: 0, y: 0 });
  const dragStart = useRef<DragStart | null>(null);

  function moveModal(event: globalThis.PointerEvent) {
    const start = dragStart.current;
    if (!start) return;
    setOffset({
      x: start.offsetX + event.clientX - start.pointerX,
      y: start.offsetY + event.clientY - start.pointerY,
    });
  }

  function stopModalDrag() {
    dragStart.current = null;
    window.removeEventListener("pointermove", moveModal);
    window.removeEventListener("pointerup", stopModalDrag);
    window.removeEventListener("pointercancel", stopModalDrag);
  }

  const resetModalPosition = useCallback(() => {
    setOffset({ x: 0, y: 0 });
    dragStart.current = null;
  }, []);

  const startModalDrag = useCallback(
    (event: PointerEvent<HTMLElement>) => {
      if (event.button !== 0) return;
      dragStart.current = {
        pointerX: event.clientX,
        pointerY: event.clientY,
        offsetX: offset.x,
        offsetY: offset.y,
      };
      window.addEventListener("pointermove", moveModal);
      window.addEventListener("pointerup", stopModalDrag);
      window.addEventListener("pointercancel", stopModalDrag);
    },
    [offset.x, offset.y],
  );

  const modalStyle = useMemo<CSSProperties>(
    () => ({
      transform: `translate(${offset.x}px, ${offset.y}px)`,
    }),
    [offset.x, offset.y],
  );

  return {
    modalStyle,
    resetModalPosition,
    startModalDrag,
  };
}
