export function isOutsidePopoverInteraction(
  event: Pick<Event, "type" | "target" | "composedPath">,
  trigger: HTMLElement | null,
  popup: HTMLElement | null,
): boolean {
  if (!trigger || !popup || !event.target) return false;
  const document = popup.ownerDocument;
  // WebKit can focus the document when a mouse-clicked button becomes disabled.
  // Only a real outside control (or an outside pointer press) should dismiss it.
  if (
    event.type === "focusin" &&
    (event.target === document.body ||
      event.target === document.documentElement)
  )
    return false;
  const path = event.composedPath();
  return ![trigger, popup].some(
    (element) =>
      path.includes(element) || element.contains(event.target as Node),
  );
}
