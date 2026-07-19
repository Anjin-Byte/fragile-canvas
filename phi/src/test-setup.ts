import "@testing-library/jest-dom/vitest";

// happy-dom does not implement ResizeObserver. Provide a mock that records
// instances so tests can push container sizes:
//   triggerResizeObservers(800, 600)
class ResizeObserverMock implements ResizeObserver {
  static instances: ResizeObserverMock[] = [];
  #callback: ResizeObserverCallback;
  #targets = new Set<Element>();

  constructor(callback: ResizeObserverCallback) {
    this.#callback = callback;
    ResizeObserverMock.instances.push(this);
  }

  observe(target: Element): void {
    this.#targets.add(target);
  }

  unobserve(target: Element): void {
    this.#targets.delete(target);
  }

  disconnect(): void {
    this.#targets.clear();
  }

  trigger(width: number, height: number): void {
    const entries = [...this.#targets].map(
      (target) =>
        ({
          target,
          contentRect: { width, height, top: 0, left: 0, bottom: height, right: width, x: 0, y: 0 },
        }) as ResizeObserverEntry,
    );
    if (entries.length > 0) this.#callback(entries, this);
  }
}

(globalThis as Record<string, unknown>).ResizeObserver = ResizeObserverMock;
(globalThis as Record<string, unknown>).triggerResizeObservers = (
  width: number,
  height: number,
) => {
  for (const ro of ResizeObserverMock.instances) ro.trigger(width, height);
};

// happy-dom does not implement the Web Animations API.
// Svelte transitions (slide, fade, etc.) call element.animate() — stub it so
// tests involving animated state transitions don't throw.
if (typeof Element !== "undefined" && !Element.prototype.animate) {
  Element.prototype.animate = function () {
    return {
      cancel: () => {},
      finish: () => {},
      finished: Promise.resolve(undefined),
      addEventListener: () => {},
      removeEventListener: () => {},
    } as unknown as Animation;
  };
}
