/**
 * `toHaveNoViolations`, declared for Vitest 3.
 *
 * `vitest-axe/extend-expect` augments `declare global { namespace Vi { interface Assertion
 * ... } }`, which is Vitest 1's shape. Vitest 3 resolves matchers off the `vitest` module's
 * own `Assertion`, so the shipped augmentation applies to nothing and every
 * `expect(await axe(...)).toHaveNoViolations()` is a type error while passing at runtime.
 *
 * The matcher itself is registered in `test/setup.ts` through `expect.extend(axeMatchers)`.
 * This file only tells the compiler it exists.
 */
import 'vitest';

declare module 'vitest' {
  interface Assertion {
    toHaveNoViolations(): void;
  }
  interface AsymmetricMatchersContaining {
    toHaveNoViolations(): void;
  }
}
