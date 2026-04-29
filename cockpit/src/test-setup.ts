// vitest setup: polyfill window.localStorage with an in-memory store.
// jsdom's Storage implementation is unreliable across vitest worker
// configurations; an explicit polyfill keeps tests deterministic.

class MemoryStorage implements Storage {
  private store: Record<string, string> = {};

  get length(): number {
    return Object.keys(this.store).length;
  }

  clear(): void {
    this.store = {};
  }

  getItem(key: string): string | null {
    return Object.prototype.hasOwnProperty.call(this.store, key) ? this.store[key] : null;
  }

  key(index: number): string | null {
    return Object.keys(this.store)[index] ?? null;
  }

  removeItem(key: string): void {
    delete this.store[key];
  }

  setItem(key: string, value: string): void {
    this.store[key] = String(value);
  }
}

const memoryStorage = new MemoryStorage();

Object.defineProperty(globalThis, "localStorage", {
  configurable: true,
  value: memoryStorage,
});
Object.defineProperty(globalThis, "sessionStorage", {
  configurable: true,
  value: new MemoryStorage(),
});
if (typeof window !== "undefined") {
  Object.defineProperty(window, "localStorage", {
    configurable: true,
    value: memoryStorage,
  });
  Object.defineProperty(window, "sessionStorage", {
    configurable: true,
    value: new MemoryStorage(),
  });
}
