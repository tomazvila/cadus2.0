/** A reply the test releases by hand, so a continuation lands exactly when the test says. */
export interface Held<T> {
  promise: Promise<T>;
  release: (value: T) => void;
}

export function held<T>(): Held<T> {
  let release!: (value: T) => void;
  const promise = new Promise<T>((r) => { release = r; });
  return { promise, release: (value: T) => release(value) };
}
