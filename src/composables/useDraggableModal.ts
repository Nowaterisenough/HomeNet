import { computed, ref } from "vue";

export function useDraggableModal() {
  const offsetX = ref(0);
  const offsetY = ref(0);
  const dragStart = ref<{ pointerX: number; pointerY: number; offsetX: number; offsetY: number } | null>(
    null,
  );

  const modalStyle = computed(() => ({
    transform: `translate(${offsetX.value}px, ${offsetY.value}px)`,
  }));

  function resetModalPosition() {
    offsetX.value = 0;
    offsetY.value = 0;
    dragStart.value = null;
  }

  function stopModalDrag() {
    dragStart.value = null;
    window.removeEventListener("pointermove", moveModal);
    window.removeEventListener("pointerup", stopModalDrag);
    window.removeEventListener("pointercancel", stopModalDrag);
  }

  function moveModal(event: PointerEvent) {
    const start = dragStart.value;
    if (!start) return;

    offsetX.value = start.offsetX + event.clientX - start.pointerX;
    offsetY.value = start.offsetY + event.clientY - start.pointerY;
  }

  function startModalDrag(event: PointerEvent) {
    if (event.button !== 0) return;

    dragStart.value = {
      pointerX: event.clientX,
      pointerY: event.clientY,
      offsetX: offsetX.value,
      offsetY: offsetY.value,
    };
    window.addEventListener("pointermove", moveModal);
    window.addEventListener("pointerup", stopModalDrag);
    window.addEventListener("pointercancel", stopModalDrag);
  }

  return {
    modalStyle,
    resetModalPosition,
    startModalDrag,
  };
}
