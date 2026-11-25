// Listen for visibility, not focus: a visible window may be behind another app.
export function whilePageVisible(start: () => () => void) {
  let stop: (() => void) | undefined;
  function sync() {
    if (document.hidden) {
      stop?.();
      stop = undefined;
    } else if (!stop) {
      stop = start();
    }
  }
  document.addEventListener("visibilitychange", sync);
  sync();
  return () => {
    document.removeEventListener("visibilitychange", sync);
    stop?.();
  };
}

export function pollWhileVisible(refresh: () => void, interval: number) {
  return whilePageVisible(() => {
    refresh();
    const timer = window.setInterval(refresh, interval);
    return () => window.clearInterval(timer);
  });
}
